// Codex CLI TUI session handling for account switching.
//
// A running `codex` TUI keeps the previous ChatGPT account in memory only when it uses an
// embedded server (`--no-daemon`, `--oss`, `-p`, `--search`, custom `-c` keys, ...). TUIs on
// the managed app-server daemon reconnect by themselves after the daemon restarts, so they
// are left open. Embedded sessions are closed gracefully (Ctrl+C written into their console)
// and, after the switch, `codex resume <id>` is typed back into the same shell. TUIs started
// with `--remote` talk to a remote server and are never touched.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::vault;

const VALUE_FLAGS: &[&str] = &[
    "-c",
    "--config",
    "-m",
    "--model",
    "-p",
    "--profile",
    "-s",
    "--sandbox",
    "-a",
    "--ask-for-approval",
    "-C",
    "--cd",
    "-i",
    "--image",
    "--add-dir",
    "--enable",
    "--disable",
    "--remote",
    "--remote-auth-token-env",
    "--local-provider",
];
const KEEP_BOOL_FLAGS: &[&str] = &[
    "--no-daemon",
    "--oss",
    "--search",
    "--full-auto",
    "--yolo",
    "--approve-for-me",
    "--dangerously-bypass-approvals-and-sandbox",
    "--dangerously-bypass-hook-trust",
    "--no-alt-screen",
];
const NON_TUI_SUBCOMMANDS: &[&str] = &[
    "exec",
    "e",
    "review",
    "login",
    "logout",
    "mcp",
    "mcp-server",
    "app-server",
    "app",
    "completion",
    "sandbox",
    "debug",
    "apply",
    "a",
    "cloud",
    "features",
    "help",
    "proto",
    "exec-server",
    "plugin",
    "update",
    "doctor",
    "remote-control",
    "archive",
    "delete",
    "unarchive",
    "responses-api-proxy",
];
const CTRL_C_PRESSES: usize = 6;
const CTRL_C_GAP: Duration = Duration::from_millis(400);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
// `daemon stop` drains running turns first (default 60 s, configurable up to 300 s), then
// force-kills after 10 s. `start` is quick.
const DAEMON_STOP_TIMEOUT: Duration = Duration::from_secs(320);
const DAEMON_START_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Result of parsing the arguments that follow the `codex` binary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Invocation {
    pub is_tui: bool,
    pub session_id: Option<String>,
    pub flags: Vec<String>,
    pub dropped_flags: Vec<String>,
    /// A flag in the invocation makes the TUI use its own embedded server.
    pub forces_embedded: bool,
    /// Started with `--remote`.
    pub remote: bool,
}

#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub cmd: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub start_time: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub codex_pid: u32,
    pub launcher_pid: Option<u32>,
    pub shell_pid: Option<u32>,
    pub cwd: Option<PathBuf>,
    pub start_time: u64,
    pub session_id: Option<String>,
    pub flags: Vec<String>,
    pub dropped_flags: Vec<String>,
    pub forces_embedded: bool,
    pub remote: bool,
}

impl Session {
    pub fn restartable(&self) -> bool {
        self.shell_pid.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct Rollout {
    pub id: String,
    /// UUID taken from the file name (`rollout-<ts>-<uuid>.jsonl`).
    pub file_id: Option<String>,
    pub cwd: String,
    pub originator: String,
    pub modified: u64,
}

/// A session whose console accepted Ctrl+C and exited.
#[derive(Debug)]
struct Closing {
    shell_pid: u32,
    session: Session,
    screen_id: Option<String>,
}

#[derive(Debug)]
struct Closed {
    shell_pid: u32,
    command: String,
    dropped: usize,
}

/// Sessions closed by `prepare_switch`, waiting for `finish_switch`.
#[derive(Debug, Default)]
pub struct Prepared {
    closed: Vec<Closed>,
    failed: usize,
    skipped: usize,
    restart: bool,
    daemon_sessions: usize,
    /// Set when a daemon stop was attempted; `finish_switch` then always tries `start`.
    daemon_exe: Option<PathBuf>,
    daemon_stopped: bool,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub closed: usize,
    pub restarted: usize,
    pub failed: usize,
    pub skipped: usize,
    pub daemon_restarted: bool,
    /// Daemon sessions that reconnect to the new account by themselves.
    pub reconnecting: usize,
    /// Flags that could not be carried over, across the sessions that were relaunched.
    pub dropped_flags: usize,
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// Tokens that are safe to type into any shell unquoted.
fn is_safe_value(v: &str) -> bool {
    !v.is_empty()
        && v.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '=' | '/' | '\\' | '-')
        })
}

/// Lower-case executable stem: `C:\\x\\Codex.EXE` -> `codex`.
fn exe_stem(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base).to_string()
}

fn args_after_binary(cmd: &[String]) -> Vec<String> {
    cmd.get(1..).unwrap_or(&[]).to_vec()
}

fn strip_quote_pair(v: &str) -> Option<&str> {
    let first = *v.as_bytes().first()?;
    let last = *v.as_bytes().last()?;
    (v.len() >= 2 && (first == b'\'' || first == b'"') && first == last).then(|| &v[1..v.len() - 1])
}

/// `key='inner'` / `key="inner"` / `'inner'` -> unquoted form, otherwise unchanged.
fn unquote_config(value: &str) -> String {
    if let Some(inner) = strip_quote_pair(value) {
        return inner.to_string();
    }
    if let Some((key, rest)) = value.split_once('=') {
        if let Some(inner) = strip_quote_pair(rest) {
            return format!("{key}={inner}");
        }
    }
    value.to_string()
}

/// Flags that always run the TUI on an embedded server.
const EMBEDDED_FLAGS: &[&str] = &[
    "--no-daemon",
    "--oss",
    "-p",
    "--profile",
    "--search",
    "--strict-config",
    "--dangerously-bypass-hook-trust",
];
/// `-c` keys that keep a TUI on the daemon. `features.*` additionally needs a true/false value.
const DAEMON_CONFIG_KEYS: &[&str] = &[
    "tui.fullscreen_transcript",
    "suppress_unstable_features_warning",
];

/// Whether one `-c`/`--config` value makes the TUI use an embedded server.
fn config_forces_embedded(value: &str) -> bool {
    let unquoted = unquote_config(value);
    let (key, val) = unquoted.split_once('=').unwrap_or((unquoted.as_str(), ""));
    if key.starts_with("features.") {
        !matches!(val, "true" | "false")
    } else {
        !DAEMON_CONFIG_KEYS.contains(&key)
    }
}

/// Scans every flag before `--`, including values that are later dropped as unsafe.
/// Returns `(forces_embedded, remote)`.
fn scan_triggers(args: &[String]) -> (bool, bool) {
    let (mut forces_embedded, mut remote) = (false, false);
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        i += 1;
        if arg == "--" {
            break;
        }
        if !arg.starts_with('-') || arg.len() < 2 {
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (arg, None),
        };
        forces_embedded |= EMBEDDED_FLAGS.contains(&name);
        remote |= name == "--remote";
        if VALUE_FLAGS.contains(&name) {
            let value = match inline {
                Some(v) => Some(v.to_string()),
                None => {
                    let next = args.get(i).cloned();
                    i += 1;
                    next
                }
            };
            if matches!(name, "-c" | "--config") {
                forces_embedded |= value.as_deref().is_some_and(config_forces_embedded);
            }
        }
    }
    (forces_embedded, remote)
}

/// Removes repeated flags from a preserved flag list. For `-c`/`--config` only the last value
/// of each key is kept (a wrapper re-injects them on every launch). Other repeated value flags
/// and boolean flags keep their first occurrence. The order of the rest is preserved.
fn dedupe_flags(flags: &[String]) -> Vec<String> {
    let mut items: Vec<(&str, Option<&str>)> = Vec::new();
    let mut i = 0;
    while i < flags.len() {
        let name = flags[i].as_str();
        if VALUE_FLAGS.contains(&name) && i + 1 < flags.len() {
            items.push((name, Some(flags[i + 1].as_str())));
            i += 2;
        } else {
            items.push((name, None));
            i += 1;
        }
    }
    let is_config = |n: &str| matches!(n, "-c" | "--config");
    let config_key = |v: &str| v.split_once('=').map_or(v, |(k, _)| k).to_string();
    let mut out = Vec::new();
    for (idx, &(name, value)) in items.iter().enumerate() {
        let repeated = if is_config(name) {
            let key = config_key(value.unwrap_or(""));
            items[idx + 1..]
                .iter()
                .any(|&(n, v)| is_config(n) && config_key(v.unwrap_or("")) == key)
        } else {
            items[..idx].iter().any(|&(n, v)| n == name && v == value)
        };
        if repeated {
            continue;
        }
        out.push(name.to_string());
        if let Some(v) = value {
            out.push(v.to_string());
        }
    }
    out
}

/// Parses the invocation and adds the classification flags and the flag de-duplication.
pub fn parse_invocation(args: &[String]) -> Invocation {
    let mut inv = parse_tokens(args);
    let (forces_embedded, remote) = scan_triggers(args);
    inv.forces_embedded = forces_embedded;
    inv.remote = remote;
    inv.flags = dedupe_flags(&inv.flags);
    inv
}

fn parse_tokens(args: &[String]) -> Invocation {
    let mut inv = Invocation {
        is_tui: true,
        ..Default::default()
    };
    let mut i = 0;
    let mut ended = false; // after "--" every token is positional
    let mut subcommand: Option<&str> = None;
    let mut id_seen = false;

    while i < args.len() {
        let arg = args[i].as_str();
        i += 1;

        if !ended && arg == "--" {
            ended = true;
            continue;
        }

        if !ended && arg.starts_with('-') && arg.len() > 1 {
            if matches!(arg, "--help" | "-h" | "--version" | "-V") {
                inv.is_tui = false;
                return inv;
            }
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (arg, None),
            };
            if VALUE_FLAGS.contains(&name) {
                let value = match inline {
                    Some(v) => v,
                    None => match args.get(i) {
                        Some(v) => {
                            i += 1;
                            v.clone()
                        }
                        None => break,
                    },
                };
                // Image attachments belong to the old prompt; never re-send them.
                if name == "-i" || name == "--image" {
                    continue;
                }
                let value = if matches!(name, "-c" | "--config") {
                    unquote_config(&value)
                } else {
                    value
                };
                if is_safe_value(&value) {
                    inv.flags.push(name.to_string());
                    inv.flags.push(value);
                } else {
                    inv.dropped_flags.push(name.to_string());
                }
            } else if KEEP_BOOL_FLAGS.contains(&name) {
                inv.flags.push(name.to_string());
            }
            continue;
        }

        match subcommand {
            None => {
                if NON_TUI_SUBCOMMANDS.contains(&arg) {
                    inv.is_tui = false;
                    return inv;
                }
                if arg == "resume" || arg == "fork" {
                    subcommand = Some(if arg == "resume" { "resume" } else { "fork" });
                    continue;
                }
                // A prompt: an interactive session, and the rest belongs to the prompt.
                break;
            }
            Some(sub) if !id_seen && is_uuid(arg) => {
                // Only `resume <id>` is resumed by id; a forked id is the parent's, so it is
                // ignored and the fork is found via the screen or the rollout fallback.
                id_seen = true;
                if sub == "resume" {
                    inv.session_id = Some(arg.to_string());
                }
            }
            // Any other positional after resume/fork is prompt text.
            Some(_) => break,
        }
    }
    inv
}

/// `codex resume <id> <flags...>`, or `codex resume <flags...>` (opens the picker).
pub fn resume_command(session: Option<&str>, flags: &[String]) -> String {
    let mut parts = vec!["codex".to_string(), "resume".to_string()];
    if let Some(id) = session.filter(|id| is_uuid(id)) {
        parts.push(id.to_string());
    }
    parts.extend(flags.iter().filter(|f| is_safe_value(f)).cloned());
    parts.join(" ")
}

/// Every UUID that follows the word `resume` on the screen, in order (duplicates kept).
pub fn find_resume_ids(screen_text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = screen_text;
    while let Some(pos) = rest.find("resume") {
        rest = &rest[pos + "resume".len()..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        // Whitespace (padding, wrapped rows) is skipped while collecting the 36-char id.
        let candidate: String = rest
            .chars()
            .filter(|c| !c.is_whitespace())
            .take(36)
            .collect();
        if is_uuid(&candidate) {
            ids.push(candidate);
        }
    }
    ids
}

/// The last id that appears more often on the screen after the event than before it.
pub fn new_resume_id(before: &str, after: &str) -> Option<String> {
    let before_ids = find_resume_ids(before);
    let after_ids = find_resume_ids(after);
    let count = |ids: &[String], id: &str| ids.iter().filter(|x| x.as_str() == id).count();
    after_ids
        .iter()
        .rev()
        .find(|id| count(&after_ids, id) > count(&before_ids, id))
        .cloned()
}

fn is_ps_command_flag(a: &str) -> bool {
    a == "-e"
        || (a.len() >= 2
            && ["-command", "-file", "-encodedcommand"]
                .iter()
                .any(|f| f.starts_with(a)))
}

/// Whether a shell process hosts an interactive prompt (and can receive typed input).
pub fn is_interactive_shell(name: &str, args: &[String]) -> bool {
    let lower: Vec<String> = args.iter().map(|a| a.to_ascii_lowercase()).collect();
    match exe_stem(name).as_str() {
        "powershell" | "pwsh" => {
            if lower
                .iter()
                .any(|a| a.len() >= 4 && "-noexit".starts_with(a.as_str()))
            {
                return true;
            }
            !lower.iter().any(|a| is_ps_command_flag(a))
        }
        "cmd" => !lower.iter().any(|a| a == "/c"),
        "bash" | "zsh" | "sh" | "fish" | "nu" => !lower.iter().any(|a| {
            a == "-c"
                || a == "--command"
                || a == "--commands"
                || (a.starts_with('-') && !a.starts_with("--") && a.contains('c'))
                || !a.starts_with('-')
        }),
        _ => false,
    }
}

fn is_posix_shell(name: &str) -> bool {
    matches!(exe_stem(name).as_str(), "bash" | "zsh" | "sh" | "fish")
}

/// POSIX shells mangle backslashes when typing, so value flags containing one are dropped.
fn drop_backslash_values(flags: &[String], dropped: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < flags.len() {
        if VALUE_FLAGS.contains(&flags[i].as_str()) && i + 1 < flags.len() {
            if flags[i + 1].contains('\\') {
                dropped.push(flags[i].clone());
            } else {
                out.push(flags[i].clone());
                out.push(flags[i + 1].clone());
            }
            i += 2;
        } else {
            out.push(flags[i].clone());
            i += 1;
        }
    }
    out
}

fn norm_path(p: &str) -> String {
    p.trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

/// Fallback id from rollout files, used only when the screen and arguments give no id.
pub fn match_rollout(
    session: &Session,
    all_sessions: &[Session],
    rollouts: &[Rollout],
) -> Option<String> {
    let cwd = norm_path(&session.cwd.as_ref()?.to_string_lossy());
    let same_cwd = |s: &Session| {
        s.cwd
            .as_ref()
            .is_some_and(|c| norm_path(&c.to_string_lossy()) == cwd)
    };
    if all_sessions.iter().filter(|s| same_cwd(s)).count() != 1 {
        return None;
    }
    let mut candidates = rollouts.iter().filter(|r| {
        r.originator == "codex-tui" && norm_path(&r.cwd) == cwd && r.modified >= session.start_time
    });
    let first = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    Some(first.id.clone())
}

/// Chooses the id to resume. A screen id is trusted only when a rollout written since the
/// session started carries it (or it is the session's own explicit id); this rejects old
/// ids left in scrollback.
pub fn choose_resume_id(
    session: &Session,
    all_sessions: &[Session],
    screen_id: Option<&str>,
    rollouts: &[Rollout],
) -> Option<String> {
    let fresh = |id: &str| {
        rollouts.iter().any(|r| {
            r.modified >= session.start_time && (r.id == id || r.file_id.as_deref() == Some(id))
        })
    };
    if let Some(id) = screen_id.filter(|id| session.session_id.as_deref() == Some(*id) || fresh(id))
    {
        return Some(id.to_string());
    }
    session
        .session_id
        .clone()
        .or_else(|| match_rollout(session, all_sessions, rollouts))
}

/// Plans running TUI sessions from a process snapshot (pure, platform independent).
pub fn plan_sessions(processes: &[ProcInfo]) -> Vec<Session> {
    let by_pid: HashMap<u32, &ProcInfo> = processes.iter().map(|p| (p.pid, p)).collect();
    // A parent pid is only trusted if it started no later than the child (guards pid reuse).
    let parent_of = |p: &ProcInfo| {
        p.parent
            .and_then(|pid| by_pid.get(&pid).copied())
            .filter(|par| par.start_time <= p.start_time)
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for p in processes {
        if exe_stem(&p.name) != "codex"
            || seen.contains(&p.pid)
            || has_chatgpt_ancestor(p, &by_pid)
            || p.cmd.iter().any(|a| a.starts_with("--type="))
        {
            continue;
        }
        let args = args_after_binary(&p.cmd);
        let inv = parse_invocation(&args);
        if !inv.is_tui {
            continue;
        }
        seen.insert(p.pid);

        let (launcher, shell_candidate) = match parent_of(p) {
            Some(node) if is_codex_js_launcher(node) => (Some(node), parent_of(node)),
            other => (None, other),
        };
        let shell = shell_candidate
            .filter(|sh| is_interactive_shell(&sh.name, &args_after_binary(&sh.cmd)));
        let (flags, dropped_flags) = match shell {
            Some(sh) if is_posix_shell(&sh.name) => {
                let mut dropped = inv.dropped_flags;
                let flags = drop_backslash_values(&inv.flags, &mut dropped);
                (flags, dropped)
            }
            _ => (inv.flags, inv.dropped_flags),
        };

        out.push(Session {
            codex_pid: p.pid,
            launcher_pid: launcher.map(|l| l.pid),
            shell_pid: shell.map(|sh| sh.pid),
            cwd: p
                .cwd
                .clone()
                .or_else(|| launcher.and_then(|l| l.cwd.clone())),
            start_time: p.start_time,
            session_id: inv.session_id,
            flags,
            dropped_flags,
            forces_embedded: inv.forces_embedded,
            remote: inv.remote,
        });
    }
    out
}

fn is_codex_js_launcher(p: &ProcInfo) -> bool {
    exe_stem(&p.name) == "node"
        && p.cmd
            .join(" ")
            .replace('\\', "/")
            .to_ascii_lowercase()
            .contains("@openai/codex/bin/codex.js")
}

fn has_chatgpt_ancestor(p: &ProcInfo, by_pid: &HashMap<u32, &ProcInfo>) -> bool {
    let mut child: &ProcInfo = p;
    for _ in 0..64 {
        let Some(parent) = child
            .parent
            .and_then(|pid| by_pid.get(&pid).copied())
            .filter(|par| par.start_time <= child.start_time)
        else {
            return false;
        };
        if exe_stem(&parent.name) == "chatgpt" {
            return true;
        }
        child = parent;
    }
    false
}

/// The managed app-server daemon's executable, if one is running.
fn managed_daemon(procs: &[ProcInfo]) -> Option<PathBuf> {
    procs
        .iter()
        .find(|p| {
            exe_stem(&p.name) == "codex"
                && p.cmd.iter().any(|a| a == "app-server")
                && p.cmd.iter().any(|a| a == "--managed-daemon")
        })
        .and_then(|p| p.cmd.first())
        .map(PathBuf::from)
}

/// How a running TUI is attached. It decides whether a switch may touch the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Attach {
    /// Uses the managed daemon: reconnects by itself after a daemon restart.
    Daemon,
    /// Has its own server: keeps the old account until it is relaunched.
    Embedded,
    /// Talks to a remote server: never touched.
    Remote,
}

fn attach_mode(s: &Session, daemon_running: bool) -> Attach {
    if s.remote {
        Attach::Remote
    } else if daemon_running && !s.forces_embedded {
        Attach::Daemon
    } else {
        Attach::Embedded
    }
}

/// What a switch does with the planned sessions.
#[derive(Debug, Default, PartialEq, Eq)]
struct SessionPlan {
    /// Indices of embedded sessions to close and resume (restart only).
    close: Vec<usize>,
    /// Non-remote sessions that are not closed: not restartable, or restart is off.
    skipped: usize,
    /// Daemon sessions. They are never closed.
    daemon: usize,
}

fn plan_switch(sessions: &[Session], daemon_running: bool, restart: bool) -> SessionPlan {
    let mut plan = SessionPlan::default();
    for (i, s) in sessions.iter().enumerate() {
        match attach_mode(s, daemon_running) {
            Attach::Remote => {}
            Attach::Daemon => {
                plan.daemon += 1;
                if !restart {
                    plan.skipped += 1;
                }
            }
            Attach::Embedded if restart && s.restartable() => plan.close.push(i),
            Attach::Embedded => plan.skipped += 1,
        }
    }
    plan
}

/// The daemon keeps the old account for new sessions. It is stopped on a switch when sessions
/// are restarted, or when no daemon session would break.
fn daemon_should_stop(restart: bool, daemon_sessions: usize) -> bool {
    restart || daemon_sessions == 0
}

/// `(reconnecting, extra failed)` for the daemon part of a switch. Daemon sessions count as
/// reconnected only when the daemon restart fully succeeded. If they were meant to be restarted
/// and that failed, they count as failed.
fn daemon_outcome(restart: bool, daemon_sessions: usize, daemon_restarted: bool) -> (usize, usize) {
    if daemon_restarted {
        (daemon_sessions, 0)
    } else if restart {
        (0, daemon_sessions)
    } else {
        (0, 0)
    }
}

// ---------------------------------------------------------------------------
// System layer
// ---------------------------------------------------------------------------

fn snapshot() -> Vec<ProcInfo> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::Always)
            .with_cwd(UpdateKind::Always),
    );
    sys.processes()
        .iter()
        .filter(|(_, p)| p.thread_kind().is_none())
        .map(|(pid, p)| ProcInfo {
            pid: pid.as_u32(),
            parent: p.parent().map(|x| x.as_u32()),
            name: p.name().to_string_lossy().into_owned(),
            cmd: p
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect(),
            cwd: p.cwd().map(Path::to_path_buf),
            start_time: p.start_time(),
        })
        .collect()
}

fn is_alive(pid: u32) -> bool {
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(pid).is_some()
}

pub fn status() -> Value {
    let procs = snapshot();
    // Remote TUIs are not local sessions: a switch never touches them, so they are not counted.
    let sessions: Vec<Session> = plan_sessions(&procs)
        .into_iter()
        .filter(|s| !s.remote)
        .collect();
    json!({
        "running": sessions.len(),
        "restartable": sessions.iter().filter(|s| s.restartable()).count(),
        "restartSupported": cfg!(windows),
        "daemonRunning": managed_daemon(&procs).is_some(),
    })
}

/// Closes embedded restartable sessions (only when `restart` is true and supported) and stops
/// the managed daemon when needed. Daemon and remote sessions are never closed. Never kills a
/// process: a session that does not exit is failed.
pub fn prepare_switch(restart: bool) -> Prepared {
    let procs = snapshot();
    let sessions = plan_sessions(&procs);
    let restart = restart && cfg!(windows);
    let daemon = managed_daemon(&procs);
    let plan = plan_switch(&sessions, daemon.is_some(), restart);
    let mut prepared = Prepared {
        skipped: plan.skipped,
        restart,
        daemon_sessions: plan.daemon,
        ..Default::default()
    };

    if restart {
        let to_close: Vec<&Session> = plan.close.iter().map(|&i| &sessions[i]).collect();
        let closings = close_sessions(&to_close, &mut prepared);
        // Rollouts are read only after every session has exited, since they are flushed on exit.
        if let Some(min_start) = closings.iter().map(|c| c.session.start_time).min() {
            let rollouts = read_rollouts(min_start);
            for c in closings {
                let id = choose_resume_id(&c.session, &sessions, c.screen_id.as_deref(), &rollouts);
                prepared.closed.push(Closed {
                    shell_pid: c.shell_pid,
                    command: resume_command(id.as_deref(), &c.session.flags),
                    dropped: c.session.dropped_flags.len(),
                });
            }
        }
    }

    if let Some(exe) = daemon.filter(|_| daemon_should_stop(restart, plan.daemon)) {
        prepared.daemon_stopped = run_daemon(&exe, "stop", DAEMON_STOP_TIMEOUT);
        prepared.daemon_exe = Some(exe);
    }
    prepared
}

/// Restarts the daemon if a stop was attempted, then types the resume command into each closed
/// session's shell. Safe to call on the error path.
pub fn finish_switch(prepared: Prepared) -> Report {
    let mut report = Report {
        closed: prepared.closed.len(),
        failed: prepared.failed,
        skipped: prepared.skipped,
        ..Default::default()
    };
    if let Some(exe) = &prepared.daemon_exe {
        // `start` is tried even when the stop failed, so the daemon is not left down.
        let started = run_daemon(exe, "start", DAEMON_START_TIMEOUT);
        report.daemon_restarted = prepared.daemon_stopped && started;
    }
    let (reconnecting, daemon_failed) =
        daemon_outcome(prepared.restart, prepared.daemon_sessions, report.daemon_restarted);
    report.reconnecting = reconnecting;
    report.failed += daemon_failed;
    for closed in &prepared.closed {
        thread::sleep(Duration::from_millis(500));
        match type_command(closed.shell_pid, &closed.command) {
            Ok(()) => {
                report.restarted += 1;
                report.dropped_flags += closed.dropped;
            }
            Err(_) => report.failed += 1,
        }
    }
    report
}

fn run_daemon(exe: &Path, action: &str, timeout: Duration) -> bool {
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["app-server", "daemon", action])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            _ => return false, // timed out or unreadable: leave the child alone
        }
    }
}

fn close_sessions(sessions: &[&Session], prepared: &mut Prepared) -> Vec<Closing> {
    let mut closings = Vec::new();
    for &s in sessions {
        let Some(shell) = s.shell_pid else {
            prepared.skipped += 1;
            continue;
        };
        let before = match console::read_screen(shell) {
            Ok(text) => text,
            Err(_) => {
                prepared.failed += 1;
                continue;
            }
        };
        if !close_gracefully(s) {
            prepared.failed += 1;
            continue;
        }
        let after = console::read_screen(shell).unwrap_or_default();
        closings.push(Closing {
            shell_pid: shell,
            session: s.clone(),
            screen_id: new_resume_id(&before, &after),
        });
    }
    closings
}

fn close_gracefully(s: &Session) -> bool {
    let Some(shell) = s.shell_pid else {
        return false;
    };
    let exited = || !is_alive(s.codex_pid) && s.launcher_pid.is_none_or(|p| !is_alive(p));
    let deadline = Instant::now() + CLOSE_TIMEOUT;
    for _ in 0..CTRL_C_PRESSES {
        if !is_alive(s.codex_pid) || !is_alive(shell) {
            break;
        }
        if console::send_ctrl_c(shell).is_err() {
            return false;
        }
        thread::sleep(CTRL_C_GAP);
    }
    while !exited() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
    true
}

fn type_command(shell_pid: u32, command: &str) -> Result<(), String> {
    if !is_alive(shell_pid) {
        return Err("shell is gone".into());
    }
    console::type_line(shell_pid, command)
}

fn codex_home() -> Option<PathBuf> {
    match std::env::var_os("CODEX_HOME") {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => vault::codex_dir().ok(),
    }
}

/// Reads the first line of rollout files modified at or after `min_modified`.
fn read_rollouts(min_modified: u64) -> Vec<Rollout> {
    let mut out = Vec::new();
    if let Some(home) = codex_home() {
        walk_rollouts(&home.join("sessions"), 0, min_modified, &mut out);
    }
    out
}

fn walk_rollouts(dir: &Path, depth: u8, min_modified: u64, out: &mut Vec<Rollout>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let path = entry.path();
        if meta.is_dir() {
            walk_rollouts(&path, depth + 1, min_modified, out);
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("rollout-") && name.ends_with(".jsonl")) {
            continue;
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        if modified < min_modified {
            continue;
        }
        if let Some(r) = read_rollout_meta(&path, modified) {
            out.push(r);
        }
    }
}

fn file_uuid(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let tail = stem.get(stem.len().checked_sub(36)?..)?;
    is_uuid(tail).then(|| tail.to_string())
}

fn read_rollout_meta(path: &Path, modified: u64) -> Option<Rollout> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).ok()?;
    let mut line = String::new();
    std::io::BufReader::new(file).read_line(&mut line).ok()?;
    let v: Value = serde_json::from_str(&line).ok()?;
    if v["type"].as_str() != Some("session_meta") {
        return None;
    }
    let payload = &v["payload"];
    Some(Rollout {
        id: payload["id"].as_str()?.to_string(),
        file_id: file_uuid(path),
        cwd: payload["cwd"].as_str()?.to_string(),
        originator: payload["originator"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        modified,
    })
}

// ---------------------------------------------------------------------------
// Console I/O
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod console {
    use std::sync::Mutex;

    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        AttachConsole, FreeConsole, GetConsoleScreenBufferInfo, ReadConsoleOutputCharacterW,
        SetConsoleCtrlHandler, WriteConsoleInputW, CONSOLE_SCREEN_BUFFER_INFO, COORD, INPUT_RECORD,
        INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, LEFT_CTRL_PRESSED,
    };

    /// Only one process-wide console attachment at a time.
    static CONSOLE_LOCK: Mutex<()> = Mutex::new(());
    const MAX_ROWS: i32 = 600;
    const VK_RETURN: u16 = 0x0D;
    const VK_C: u16 = 0x43;

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    /// Restores our own console state on drop: Ctrl+C handling and the attachment.
    struct Detach;

    impl Drop for Detach {
        fn drop(&mut self) {
            unsafe {
                let _ = SetConsoleCtrlHandler(None, false);
                let _ = FreeConsole();
            }
        }
    }

    fn open(name: PCWSTR) -> Result<OwnedHandle, String> {
        unsafe {
            CreateFileW(
                name,
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .map(OwnedHandle)
        .map_err(|e| format!("open console failed: {e}"))
    }

    /// Runs `f` with the console of `pid` attached. Always detaches afterwards.
    fn with_console<T>(
        pid: u32,
        f: impl FnOnce(HANDLE, HANDLE) -> Result<T, String>,
    ) -> Result<T, String> {
        let _lock = CONSOLE_LOCK
            .lock()
            .map_err(|_| "console lock poisoned".to_string())?;
        unsafe {
            let _ = FreeConsole();
            AttachConsole(pid).map_err(|e| format!("attach console failed: {e}"))?;
        }
        let _detach = Detach;
        unsafe {
            let _ = SetConsoleCtrlHandler(None, true);
        }
        let input = open(w!("CONIN$"))?;
        let output = open(w!("CONOUT$"))?;
        f(input.0, output.0)
    }

    fn key(vk: u16, unit: u16, down: bool, ctrl: u32) -> INPUT_RECORD {
        INPUT_RECORD {
            EventType: KEY_EVENT as u16,
            Event: INPUT_RECORD_0 {
                KeyEvent: KEY_EVENT_RECORD {
                    bKeyDown: down.into(),
                    wRepeatCount: 1,
                    wVirtualKeyCode: vk,
                    wVirtualScanCode: 0,
                    uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                    dwControlKeyState: ctrl,
                },
            },
        }
    }

    fn write_keys(input: HANDLE, records: &[INPUT_RECORD]) -> Result<(), String> {
        let mut written = 0u32;
        unsafe { WriteConsoleInputW(input, records, &mut written) }
            .map_err(|e| format!("write console input failed: {e}"))?;
        if written as usize != records.len() {
            return Err("console input partially written".into());
        }
        Ok(())
    }

    pub fn send_ctrl_c(pid: u32) -> Result<(), String> {
        with_console(pid, |input, _| {
            write_keys(
                input,
                &[
                    key(VK_C, 3, true, LEFT_CTRL_PRESSED),
                    key(VK_C, 3, false, LEFT_CTRL_PRESSED),
                ],
            )
        })
    }

    pub fn type_line(pid: u32, text: &str) -> Result<(), String> {
        with_console(pid, |input, _| {
            let mut records = Vec::with_capacity(text.len() * 2 + 2);
            for unit in text.encode_utf16() {
                records.push(key(0, unit, true, 0));
                records.push(key(0, unit, false, 0));
            }
            records.push(key(VK_RETURN, b'\r' as u16, true, 0));
            records.push(key(VK_RETURN, b'\r' as u16, false, 0));
            write_keys(input, &records)
        })
    }

    /// Text of the last rows up to the cursor. Rows are joined without separators so that
    /// ids wrapped across rows stay intact.
    pub fn read_screen(pid: u32) -> Result<String, String> {
        with_console(pid, |_, output| {
            let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
            unsafe { GetConsoleScreenBufferInfo(output, &mut info) }
                .map_err(|e| format!("screen info failed: {e}"))?;
            let width = info.dwSize.X.max(1) as usize;
            let cursor = info.dwCursorPosition.Y;
            let top = (cursor as i32 - MAX_ROWS).max(0) as i16;
            let mut text = String::new();
            let mut buf = vec![0u16; width];
            for row in top..=cursor {
                let mut read = 0u32;
                unsafe {
                    ReadConsoleOutputCharacterW(output, &mut buf, COORD { X: 0, Y: row }, &mut read)
                }
                .map_err(|e| format!("read console failed: {e}"))?;
                text.push_str(&String::from_utf16_lossy(
                    &buf[..(read as usize).min(width)],
                ));
            }
            Ok(text)
        })
    }
}

#[cfg(not(windows))]
mod console {
    const UNSUPPORTED: &str = "restart is only supported on Windows";
    pub fn read_screen(_pid: u32) -> Result<String, String> {
        Err(UNSUPPORTED.into())
    }
    pub fn send_ctrl_c(_pid: u32) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
    pub fn type_line(_pid: u32, _text: &str) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "019a1b2c-3d4e-5f60-8172-93a4b5c6d7e8";
    const UUID2: &str = "11111111-2222-3333-4444-555555555555";

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn proc(
        pid: u32,
        parent: Option<u32>,
        name: &str,
        cmd: &[&str],
        cwd: Option<&str>,
    ) -> ProcInfo {
        ProcInfo {
            pid,
            parent,
            name: name.into(),
            cmd: args(cmd),
            cwd: cwd.map(PathBuf::from),
            start_time: 1_000 + pid as u64,
        }
    }

    fn session(start: u64, cwd: Option<&str>, session_id: Option<&str>) -> Session {
        Session {
            codex_pid: 1,
            launcher_pid: None,
            shell_pid: Some(2),
            cwd: cwd.map(PathBuf::from),
            start_time: start,
            session_id: session_id.map(str::to_string),
            flags: vec![],
            dropped_flags: vec![],
            forces_embedded: false,
            remote: false,
        }
    }

    fn rollout(
        id: &str,
        file_id: Option<&str>,
        cwd: &str,
        originator: &str,
        modified: u64,
    ) -> Rollout {
        Rollout {
            id: id.into(),
            file_id: file_id.map(str::to_string),
            cwd: cwd.into(),
            originator: originator.into(),
            modified,
        }
    }

    #[test]
    fn invocation_tui_and_non_tui() {
        assert!(parse_invocation(&args(&[])).is_tui);
        assert!(parse_invocation(&args(&["fix the bug"])).is_tui);
        assert!(parse_invocation(&args(&["-m", "gpt-5", "resume", "--last"])).is_tui);
        assert!(!parse_invocation(&args(&["exec", "x"])).is_tui);
        assert!(!parse_invocation(&args(&["--model=gpt-5", "app-server"])).is_tui);
        assert!(!parse_invocation(&args(&["--version"])).is_tui);
        assert!(!parse_invocation(&args(&["-h"])).is_tui);
        assert!(!parse_invocation(&args(&["login"])).is_tui);
    }

    #[test]
    fn invocation_resume_id_and_flags() {
        let inv = parse_invocation(&args(&[
            "--no-daemon",
            "-m",
            "gpt-5",
            "resume",
            UUID,
            "--yolo",
            "--last",
        ]));
        assert!(inv.is_tui);
        assert_eq!(inv.session_id.as_deref(), Some(UUID));
        assert_eq!(inv.flags, args(&["--no-daemon", "-m", "gpt-5", "--yolo"]));
        assert!(inv.dropped_flags.is_empty());
    }

    #[test]
    fn invocation_fork_is_resumed_not_reforked() {
        let inv = parse_invocation(&args(&["fork", UUID, "--oss"]));
        assert!(inv.is_tui);
        assert_eq!(inv.session_id, None);
        assert_eq!(inv.flags, args(&["--oss"]));
    }

    #[test]
    fn invocation_drops_unsafe_values_and_prompts() {
        let inv = parse_invocation(&args(&[
            "-c",
            "model=x y",
            "--cd=C:\\work\\repo",
            "--image",
            "a.png",
            "--yolo",
            "fix it",
            "--oss",
        ]));
        assert!(inv.is_tui);
        assert_eq!(inv.flags, args(&["--cd", "C:\\work\\repo", "--yolo"]));
        assert_eq!(inv.dropped_flags, args(&["-c"]));
        assert_eq!(inv.session_id, None);
    }

    #[test]
    fn invocation_double_dash_makes_prompt() {
        let inv = parse_invocation(&args(&["--", "--yolo"]));
        assert!(inv.is_tui);
        assert!(inv.flags.is_empty());
    }

    #[test]
    fn config_quotes_are_unwrapped() {
        let inv = parse_invocation(&args(&[
            "-c",
            "shell_environment_policy.set.ORCA_TERMINAL_HANDLE='term_x'",
            "-c",
            "'model=gpt'",
            "-c",
            "model=\"gpt\"",
            "-c",
            "a=\"x y\"",
        ]));
        // The repeated `model=gpt` collapses to its last occurrence.
        assert_eq!(
            inv.flags,
            args(&[
                "-c",
                "shell_environment_policy.set.ORCA_TERMINAL_HANDLE=term_x",
                "-c",
                "model=gpt",
            ])
        );
        assert_eq!(inv.dropped_flags, args(&["-c"]));
    }

    #[test]
    fn repeated_flags_are_collapsed() {
        let inv = parse_invocation(&args(&[
            "-c",
            "shell_environment_policy.set.ORCA_TERMINAL_HANDLE=term_a",
            "-c",
            "model=gpt",
            "-c",
            "shell_environment_policy.set.ORCA_TERMINAL_HANDLE=term_b",
            "--yolo",
            "--yolo",
            "--add-dir",
            "C:\\a",
            "--add-dir",
            "C:\\a",
            "--add-dir",
            "C:\\b",
            "--enable",
            "x",
            "--enable",
            "x",
        ]));
        assert_eq!(
            inv.flags,
            args(&[
                "-c",
                "model=gpt",
                "-c",
                "shell_environment_policy.set.ORCA_TERMINAL_HANDLE=term_b",
                "--yolo",
                "--add-dir",
                "C:\\a",
                "--add-dir",
                "C:\\b",
                "--enable",
                "x",
            ])
        );
    }

    #[test]
    fn embedded_triggers_force_embedded() {
        let cases: &[&[&str]] = &[
            &["--no-daemon"],
            &["--oss"],
            &["-p", "work"],
            &["--profile=work"],
            &["--search"],
            &["--strict-config"],
            &["--dangerously-bypass-hook-trust"],
            &["-c", "model=gpt"],
            &["--config=model=gpt"],
            &["-c", "shell_environment_policy.set.X=1"],
            &["-c", "features.x=yes"],
            &["-c", "features.x"],
            &["-c", "'model=gpt'"],
            // Unsafe value: dropped from the flags, but still an embedded trigger.
            &["-c", "model=x y"],
        ];
        for case in cases {
            let inv = parse_invocation(&args(case));
            assert!(inv.forces_embedded, "{case:?}");
        }
    }

    #[test]
    fn daemon_compatible_flags_do_not_force_embedded() {
        let cases: &[&[&str]] = &[
            &[],
            &["-c", "features.multi_agent=true"],
            &["--config=features.x=false"],
            &["-c", "tui.fullscreen_transcript=true"],
            &["-c", "suppress_unstable_features_warning=1"],
            &["--enable", "x", "--disable", "y"],
            &["--yolo", "-m", "gpt-5"],
        ];
        for case in cases {
            let inv = parse_invocation(&args(case));
            assert!(!inv.forces_embedded, "{case:?}");
        }
    }

    #[test]
    fn remote_flag_is_detected() {
        assert!(parse_invocation(&args(&["--remote", "ws://host:4500"])).remote);
        assert!(parse_invocation(&args(&["--remote=ws://host:4500"])).remote);
        let auth = parse_invocation(&args(&["--remote-auth-token-env", "TOKEN"]));
        assert!(!auth.remote);
        assert!(!auth.forces_embedded);
    }

    #[test]
    fn attach_mode_depends_on_daemon_and_flags() {
        let plain = session(1, None, None);
        let mut forced = plain.clone();
        forced.forces_embedded = true;
        let mut remote = plain.clone();
        remote.remote = true;
        assert_eq!(attach_mode(&plain, true), Attach::Daemon);
        assert_eq!(attach_mode(&plain, false), Attach::Embedded);
        assert_eq!(attach_mode(&forced, true), Attach::Embedded);
        assert_eq!(attach_mode(&remote, true), Attach::Remote);
        assert_eq!(attach_mode(&remote, false), Attach::Remote);
    }

    #[test]
    fn switch_plan_by_restart_setting() {
        let daemon = session(1, None, None);
        let mut embedded = session(2, None, None);
        embedded.forces_embedded = true;
        let mut no_shell = embedded.clone();
        no_shell.shell_pid = None;
        let mut remote = session(4, None, None);
        remote.remote = true;
        let sessions = [daemon, embedded, no_shell, remote];

        // Restart on: only the restartable embedded session is closed; the daemon one is kept.
        assert_eq!(
            plan_switch(&sessions, true, true),
            SessionPlan {
                close: vec![1],
                skipped: 1,
                daemon: 1,
            }
        );
        // Restart off: nothing is closed; every non-remote session is skipped.
        assert_eq!(
            plan_switch(&sessions, true, false),
            SessionPlan {
                close: vec![],
                skipped: 3,
                daemon: 1,
            }
        );
        // No managed daemon: every session is embedded.
        assert_eq!(
            plan_switch(&sessions, false, true),
            SessionPlan {
                close: vec![0, 1],
                skipped: 1,
                daemon: 0,
            }
        );
    }

    #[test]
    fn daemon_outcome_reports_reconnects_and_failures() {
        assert_eq!(daemon_outcome(true, 2, true), (2, 0));
        assert_eq!(daemon_outcome(true, 2, false), (0, 2));
        assert_eq!(daemon_outcome(false, 2, false), (0, 0));
        assert_eq!(daemon_outcome(true, 0, false), (0, 0));
    }

    #[test]
    fn posix_shell_drops_backslash_values_only() {
        let codex_args = ["codex.exe", "--cd", "C:\\work", "--yolo"];
        let ps = plan_sessions(&[
            proc(
                10,
                Some(1),
                "powershell.exe",
                &["powershell.exe", "-NoExit"],
                None,
            ),
            proc(12, Some(10), "codex.exe", &codex_args, None),
        ]);
        assert_eq!(ps[0].flags, args(&["--cd", "C:\\work", "--yolo"]));
        assert!(ps[0].dropped_flags.is_empty());

        let bash = plan_sessions(&[
            proc(10, Some(1), "bash.exe", &["bash.exe", "-l"], None),
            proc(12, Some(10), "codex.exe", &codex_args, None),
        ]);
        assert_eq!(bash[0].flags, args(&["--yolo"]));
        assert_eq!(bash[0].dropped_flags, args(&["--cd"]));
    }

    #[test]
    fn resume_command_shapes() {
        let flags = args(&["--yolo", "-m", "gpt-5"]);
        assert_eq!(
            resume_command(Some(UUID), &flags),
            format!("codex resume {UUID} --yolo -m gpt-5")
        );
        assert_eq!(resume_command(None, &args(&[])), "codex resume");
        assert_eq!(
            resume_command(Some("not-an-id; rm"), &args(&[])),
            "codex resume"
        );
        assert_eq!(
            resume_command(None, &args(&["a b", "--yolo"])),
            "codex resume --yolo"
        );
    }

    #[test]
    fn finds_ids_in_screen_text() {
        let screen = format!("To continue this session, run\n   codex resume {UUID}   \nbye");
        assert_eq!(find_resume_ids(&screen), vec![UUID.to_string()]);

        // Wrapped at the console width: id split across joined rows with padding.
        let (a, b) = UUID.split_at(20);
        let wrapped = format!("codex resume {a}          {b}   ");
        assert_eq!(find_resume_ids(&wrapped), vec![UUID.to_string()]);

        assert!(find_resume_ids("codex resume").is_empty());
        assert!(find_resume_ids("resume shortid").is_empty());
    }

    #[test]
    fn new_id_is_the_newly_printed_one() {
        let before = format!("codex resume {UUID}\n");
        let after = format!("codex resume {UUID}\ncodex resume {UUID2}\n");
        assert_eq!(new_resume_id(&before, &after), Some(UUID2.to_string()));
        assert_eq!(new_resume_id(&after, &after), None);
        assert_eq!(new_resume_id("", &before), Some(UUID.to_string()));
    }

    #[test]
    fn stale_screen_id_is_rejected() {
        // Scrollback shows an old id, codex printed nothing new, and no rollout carries it.
        let s = session(1_000, Some(r"C:\repo"), None);
        let rollouts = [rollout(UUID, None, r"C:\repo", "codex-tui", 1_500)];
        let chosen = choose_resume_id(&s, std::slice::from_ref(&s), Some(UUID2), &rollouts);
        assert_eq!(
            chosen,
            Some(UUID.to_string()),
            "falls back to rollout match, not the stale id"
        );
    }

    #[test]
    fn valid_screen_id_wins_over_args_id() {
        let s = session(1_000, Some(r"C:\repo"), Some(UUID));
        let rollouts = [rollout("x", Some(UUID2), r"C:\repo", "codex-tui", 1_500)];
        assert_eq!(
            choose_resume_id(&s, &[], Some(UUID2), &rollouts),
            Some(UUID2.to_string())
        );
    }

    #[test]
    fn old_rollout_does_not_validate_screen_id() {
        let s = session(1_000, None, Some(UUID));
        let rollouts = [rollout("x", Some(UUID2), r"C:\repo", "codex-tui", 500)];
        assert_eq!(
            choose_resume_id(&s, &[], Some(UUID2), &rollouts),
            Some(UUID.to_string())
        );
    }

    #[test]
    fn screen_id_equal_to_args_id_is_accepted() {
        let s = session(1_000, None, Some(UUID));
        assert_eq!(
            choose_resume_id(&s, &[], Some(UUID), &[]),
            Some(UUID.to_string())
        );
    }

    #[test]
    fn choose_fallbacks() {
        let none = session(1_000, None, None);
        assert_eq!(choose_resume_id(&none, &[], None, &[]), None);
        let with_args = session(1_000, None, Some(UUID));
        assert_eq!(
            choose_resume_id(&with_args, &[], None, &[]),
            Some(UUID.to_string())
        );
    }

    #[test]
    fn interactive_shell_detection() {
        let ps_interactive = args(&["-NoLogo", "-NoExit", "-EncodedCommand", "abc"]);
        assert!(is_interactive_shell("powershell.exe", &ps_interactive));
        assert!(is_interactive_shell("PowerShell.EXE", &[]));
        assert!(is_interactive_shell(
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            &[]
        ));
        assert!(!is_interactive_shell(
            "powershell.exe",
            &args(&["-NoLogo", "-EncodedCommand", "abc"])
        ));
        assert!(!is_interactive_shell(
            "pwsh.exe",
            &args(&["-Command", "ls"])
        ));
        assert!(!is_interactive_shell("pwsh", &args(&["-f", "x.ps1"])));
        assert!(is_interactive_shell("cmd.exe", &[]));
        assert!(!is_interactive_shell("cmd.exe", &args(&["/c", "x"])));
        assert!(is_interactive_shell("bash.exe", &args(&["-l"])));
        assert!(!is_interactive_shell("bash.exe", &args(&["-c", "ls"])));
        assert!(!is_interactive_shell("bash", &args(&["script.sh"])));
        assert!(!is_interactive_shell("explorer.exe", &[]));
    }

    #[test]
    fn plans_restartable_standalone_and_skips_others() {
        let procs = vec![
            // Interactive PowerShell -> node launcher -> native codex.
            proc(
                10,
                Some(1),
                "powershell.exe",
                &[
                    "powershell.exe",
                    "-NoLogo",
                    "-NoExit",
                    "-EncodedCommand",
                    "x",
                ],
                Some(r"C:\repo"),
            ),
            proc(
                11,
                Some(10),
                "node.exe",
                &[
                    "node.exe",
                    r"C:\nvm\node_modules\@openai/codex/bin/codex.js",
                    "--yolo",
                ],
                None,
            ),
            proc(
                12,
                Some(11),
                "codex.exe",
                &[r"C:\nvm\codex.exe", "--yolo"],
                Some(r"C:\repo"),
            ),
            proc(13, Some(12), "codex-code-mode-host.exe", &["x"], None),
            // Non-interactive cmd /c: found but not restartable.
            proc(20, Some(1), "cmd.exe", &["cmd.exe", "/c", "run"], None),
            proc(
                21,
                Some(20),
                "codex.exe",
                &[r"C:\codex.exe", "resume", UUID],
                Some(r"D:\other"),
            ),
            // Daemon and updater: not TUI sessions.
            proc(
                30,
                Some(1),
                "codex.exe",
                &[
                    r"C:\codex.exe",
                    "app-server",
                    "--listen",
                    "unix://",
                    "--managed-daemon",
                ],
                None,
            ),
            proc(
                31,
                Some(1),
                "codex.exe",
                &[r"C:\codex.exe", "app-server", "daemon", "pid-update-loop"],
                None,
            ),
            // Under the ChatGPT desktop app: ignored.
            proc(40, Some(1), "ChatGPT.exe", &["ChatGPT.exe"], None),
            proc(
                41,
                Some(40),
                "codex.exe",
                &[r"C:\codex.exe"],
                Some(r"C:\repo"),
            ),
            // Electron helper process: ignored.
            proc(
                42,
                Some(10),
                "codex.exe",
                &[r"C:\codex.exe", "--type=renderer"],
                None,
            ),
            // Forked session: resumed without a session id.
            proc(50, Some(1), "pwsh.exe", &["pwsh.exe", "-NoExit"], None),
            proc(
                51,
                Some(50),
                "codex.exe",
                &[r"C:\codex.exe", "fork", UUID2],
                Some(r"E:\fork"),
            ),
        ];
        let sessions = plan_sessions(&procs);
        assert_eq!(sessions.len(), 3, "{sessions:?}");

        let first = &sessions[0];
        assert_eq!(first.codex_pid, 12);
        assert_eq!(first.launcher_pid, Some(11));
        assert_eq!(first.shell_pid, Some(10));
        assert!(first.restartable());
        assert_eq!(first.flags, args(&["--yolo"]));

        let standalone = &sessions[1];
        assert_eq!(standalone.codex_pid, 21);
        assert_eq!(standalone.shell_pid, None);
        assert!(!standalone.restartable());
        assert_eq!(standalone.session_id.as_deref(), Some(UUID));

        let fork = &sessions[2];
        assert_eq!(fork.codex_pid, 51);
        assert_eq!(fork.session_id, None);
        assert_eq!(fork.shell_pid, Some(50));
    }

    #[test]
    fn reused_parent_pid_is_not_trusted() {
        // The node "parent" pid was reused by a process that started after the codex child.
        let mut node = proc(
            11,
            Some(10),
            "node.exe",
            &["node.exe", "@openai/codex/bin/codex.js"],
            None,
        );
        node.start_time = 9_000;
        let mut shell = proc(
            10,
            Some(1),
            "powershell.exe",
            &["powershell.exe", "-NoExit"],
            None,
        );
        shell.start_time = 9_500;
        let codex = proc(12, Some(11), "codex.exe", &["codex.exe"], None);
        let sessions = plan_sessions(&[shell, node, codex]);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].launcher_pid, None);
        assert_eq!(sessions[0].shell_pid, None);
    }

    #[test]
    fn chatgpt_ancestor_ignores_reused_pid() {
        let mut chatgpt = proc(40, Some(1), "ChatGPT.exe", &["ChatGPT.exe"], None);
        chatgpt.start_time = 9_000;
        let codex = proc(41, Some(40), "codex.exe", &["codex.exe"], None);
        // chatgpt started after the codex child, so pid 40 cannot be its parent.
        assert!(!has_chatgpt_ancestor(
            &codex,
            &HashMap::from([(40, &chatgpt)])
        ));
        assert_eq!(plan_sessions(&[chatgpt, codex]).len(), 1);
    }

    #[test]
    fn daemon_stop_policy() {
        assert!(daemon_should_stop(true, 0));
        assert!(daemon_should_stop(true, 2));
        assert!(!daemon_should_stop(false, 1));
        assert!(daemon_should_stop(false, 0));
    }

    #[test]
    fn rollout_matching_is_strict() {
        let session = Session {
            shell_pid: Some(2),
            ..session(1_000, Some(r"C:\Repo\App"), None)
        };
        let one = vec![
            rollout(UUID, None, r"\\?\c:/repo/app/", "codex-tui", 1_500),
            rollout(UUID2, None, r"C:\other", "codex-tui", 1_500),
            rollout(
                "33333333-2222-3333-4444-555555555555",
                None,
                r"C:\Repo\App",
                "codex_vscode",
                1_500,
            ),
            rollout(
                "44444444-2222-3333-4444-555555555555",
                None,
                r"C:\Repo\App",
                "codex-tui",
                900,
            ),
        ];
        assert_eq!(
            match_rollout(&session, std::slice::from_ref(&session), &one),
            Some(UUID.to_string())
        );

        // Two candidates with the same cwd: ambiguous.
        let mut two = one.clone();
        two.push(rollout(
            "55555555-2222-3333-4444-555555555555",
            None,
            r"C:\Repo\App",
            "codex-tui",
            1_600,
        ));
        assert_eq!(
            match_rollout(&session, std::slice::from_ref(&session), &two),
            None
        );

        // Two running sessions in the same cwd: ambiguous.
        let other = Session {
            codex_pid: 9,
            ..session.clone()
        };
        assert_eq!(
            match_rollout(&session, &[session.clone(), other], &one),
            None
        );
    }

    #[test]
    fn report_serializes_camel_case() {
        let r = Report {
            closed: 2,
            restarted: 1,
            failed: 1,
            skipped: 0,
            daemon_restarted: true,
            reconnecting: 3,
            dropped_flags: 3,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["daemonRestarted"], json!(true));
        assert_eq!(v["reconnecting"], json!(3));
        assert_eq!(v["droppedFlags"], json!(3));
    }

    #[test]
    #[ignore]
    fn detect_smoke_read_only() {
        let sessions = plan_sessions(&snapshot());
        println!("status: {}", status());
        for s in &sessions {
            println!(
                "codex_pid={} shell={:?} restartable={} session_id={:?} flags={:?} dropped={:?}",
                s.codex_pid,
                s.shell_pid,
                s.restartable(),
                s.session_id,
                s.flags,
                s.dropped_flags
            );
        }
    }
}
