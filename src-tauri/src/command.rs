//! Headless command line: `list`, `current`, `switch`, `help` and `--version`.
//!
//! Two binaries use this code before Tauri starts, so no window, tray or single-instance check
//! is involved: `codexswitcher-cli` (console subsystem, the documented command line) and the GUI
//! exe `codexswitcher`, which only dispatches here for command words. Output is built as text
//! first and written once at the end, so the restart code (which attaches and detaches other
//! consoles) cannot misplace it. Settings and the 9Router password are never read or printed
//! here; token and API key fields are never serialized.

use crate::{auth, cli, desktop, router, usage, AppState, SwitchMode};
use chrono::{DateTime, Local, Utc};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::sync::Mutex;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

pub const EXIT_OK: i32 = 0;
pub const EXIT_FAILED: i32 = 1;
pub const EXIT_USAGE: i32 = 2;

/// Hidden argument: the detached helper that restarts the daemon after a switch made inside Codex.
const DAEMON_HELPER_ARG: &str = "--internal-daemon-restart";
const GENERIC_SYNC_ERROR: &str =
    "Could not sync accounts from 9Router. Check the connection settings in the app.";
const CACHE_ERROR: &str = "Could not read the encrypted account cache.";
const SYNC_WARNING: &str = "Could not refresh from 9Router; showing saved values.";
const AUTH_ERROR: &str = "Could not read auth.json in the Codex folder.";
const REFUSE_RESTART: &str = "Refusing --restart: this command runs inside a Codex or ChatGPT process, and the restart would close it. Run switch without --restart, or run it from a plain terminal.";
const MAX_ANCESTORS: usize = 64;

const COMMAND_WORDS: &[&str] = &[
    "help",
    "--help",
    "-h",
    "--version",
    "-V",
    "list",
    "current",
    "switch",
    DAEMON_HELPER_ARG,
];

const HELP: &str = r#"CodexSwitcher command line (for scripts and AI agents)

Usage: codexswitcher-cli <command> [options]
codexswitcher-cli.exe is a console program next to codexswitcher.exe. It is not on PATH:
call it by its full path.
It lists the 9Router accounts saved by the app and switches the active Codex account
(auth.json). It cannot set up 9Router or change settings, and it never prints tokens, API
keys, the router URL or the password.

Commands:
  list [--plan <plan>] [--json]  Syncs from 9Router first (the app's sync), then lists the
                                 accounts: plan, remaining usage, resets, credits, fetch time.
                                 --plan keeps one plan (case-insensitive, after the sync).
                                 --refresh is accepted and does nothing.
  current [--json]               Syncs first, then shows the account that matches auth.json.
                                 Fails when no cached account matches.
  switch <selector> [options]    Does NOT sync: uses the cached accounts. Makes one active.
  help, --help, -h               This text. Also after a command: "switch --help" only prints
                                 help and never switches.
  --version, -V                  Prints the version.

Agent workflow:
  1. codexswitcher-cli list --json [--plan plus]   (network calls: can be slow)
  2. Choose by remaining quota (see Quota). Prefer usage.stale false.
  3. Unless the user named the account, confirm it first. Then:
     codexswitcher-cli switch <email | id | accountId> --json
  4. Read the result (see "Did sessions move?").
  5. codexswitcher-cli current --json   (confirms auth.json holds that account)

Quota: usage.fiveHourRemainingPercent (5-hour window) and usage.weeklyRemainingPercent
(weekly window) are the percent LEFT (100 minus used), never the percent used. If the user
gives only a percentage that could mean either window or several accounts, ask which.

Freshness: when "refreshed" is false, "warnings" is not empty, or usage.stale is true, the
numbers are saved values. Tell the user they are old and from when (usage.fetchedAt; null
means unknown). usage null means there are no numbers at all.

Selectors: exactly one cached account must match: connection id (id), ChatGPT account id
(accountId) or email (case-insensitive). No match or several matches exit 1; several matches
list their ids, so retry with one. switch uses the cache: run list first if the account is new.

switch options (none = default):
  (none)               Writes auth.json. The previous one is copied to the backups folder of
                       the Codex folder when it existed (backupPath). If the Codex daemon
                       runs, it is restarted and daemon-connected sessions reconnect by
                       themselves. The ChatGPT desktop app is not touched. Embedded sessions
                       keep the OLD account and are counted in running.embeddedCliSessions.
                       Embedded = started with --no-daemon, --oss, -p/--profile, --search,
                       --strict-config, --dangerously-bypass-hook-trust, or a -c/--config
                       override other than tui.fullscreen_transcript,
                       suppress_unstable_features_warning and features.* set to true/false.
  --no-daemon-restart  Writes auth.json only. Nothing is stopped or started.
  --restart            Full restart, like the app. It closes sessions, so use it only when the
                       user wants every session moved. It closes the desktop app and the
                       embedded sessions that run in a console, switches, restarts the daemon,
                       resumes the closed sessions in their consoles, and reopens the desktop
                       app if it was running and its launch setting is on. Each closed session
                       gets Ctrl+C (up to 6 times) and up to 5 s to exit, so work in progress
                       can be interrupted. A session that does not exit is not killed: it stays
                       on the old account (cli.failed). Remote sessions are never touched.
                       Sessions with no restartable console are left alone (cli.skipped).
                       The VS Code extension is not covered by this code.
  Inside Codex or ChatGPT: --restart is refused (exit 1, nothing written). The default restarts
                       the daemon after this command returns (daemon.action "scheduled"; the
                       daemon stop drains running turns, then force-stops them). If the calling
                       session is embedded or the desktop app, it keeps the old account.
  --restart and --no-daemon-restart cannot be combined.

Did sessions move? Check the switch JSON:
  default, --no-daemon-restart: running.embeddedCliSessions > 0 means sessions still on the old
    account (counted after the switch). running.desktopApp true means the desktop app still
    runs the old account and may overwrite auth.json. daemon.action "failed" means the daemon
    did not come back.
  --restart: cli.skipped > 0 means sessions left running on the old account. cli.failed counts
    sessions not closed (still on the old account), closed but not resumed (restart those by
    hand), and daemon sessions if the daemon did not restart. cli.daemonRestarted and
    cli.reconnecting describe the daemon; the top-level daemon.action stays "none" in this mode.
  "warnings" repeats the main problems in plain text. "cli" is null in the other modes.

Output: --json prints one JSON object on stdout, errors included ({"ok":false,"error":"..."}).
Without it, results are text on stdout and errors go to stderr.
Exit codes:
  0  success (list/current with saved values after a failed sync is still 0)
  1  the operation failed: no or several matches, cache or auth.json unreadable, sync failed
     with no saved accounts, --restart refused, switch failed
  2  bad usage: unknown command or option, missing value, conflicting options

JSON (success; "ok":true is always added):
  list    {"refreshed","warnings":[...],"count","accounts":[ACCOUNT...]}
  current {"refreshed","warnings":[...],"account":ACCOUNT}
  switch  {"switched":{"id","email","plan","accountId"},"mode":"auth-only"|"daemon-restart"|
           "full-restart","backupPath","steps":[...],"daemon":{"action":"none"|"restarted"|
           "scheduled"|"failed","stopped","started"},"running":{"desktopApp",
           "embeddedCliSessions","daemonRunning"},"warnings":[...],"cli":null|{"closed",
           "restarted","failed","skipped","daemonRestarted","reconnecting","droppedFlags"},
           "cacheWarning"}
  version {"version"}
  ACCOUNT {"id","email","plan","accountId","active","expiresAt","usage":null|{
           "fiveHourRemainingPercent","fiveHourResetAt","weeklyRemainingPercent",
           "weeklyResetAt","limitReached","resetCredits","fetchedAt","error","stale"}}
  usage.stale false: fresh values from this run. true: saved values from an earlier sync
  (fetchedAt is when they were fetched). error is the failed fetch, null if the whole sync failed.
"#;

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Version,
    List { plan: Option<String> },
    Current,
    Switch {
        selector: String,
        restart: bool,
        no_daemon_restart: bool,
    },
    DaemonHelper,
}

#[derive(Debug, PartialEq, Eq)]
struct UsageError(String);

/// True when the arguments select the command line; false lets the GUI start.
/// Any dash-prefixed argument that is not a known command word keeps the GUI behaviour.
fn is_cli_request(args: &[String]) -> bool {
    match args.first() {
        None => false,
        Some(first) => !first.starts_with('-') || COMMAND_WORDS.contains(&first.as_str()),
    }
}

/// Runs a command-line request and returns its exit code, or None when the GUI should start.
pub fn dispatch(args: &[String]) -> Option<i32> {
    is_cli_request(args).then(|| run(args))
}

/// Entry point of the console binary `codexswitcher-cli`. Every argument list goes through the
/// parser, so no arguments print the help (exit 0) and unknown or GUI-only flags exit with 2.
pub fn run_cli(args: &[String]) -> i32 {
    run(args)
}

fn run(args: &[String]) -> i32 {
    // Captured before anything can attach or detach a console.
    let sinks = sinks::Sinks::capture();
    let output = render_request(args);
    sinks.write(&output);
    output.code
}

/// Parses, executes and renders one request without writing it anywhere.
fn render_request(args: &[String]) -> Output {
    let json = args.iter().any(|arg| arg == "--json");
    let outcome = match parse(args) {
        Ok(command) => execute(command),
        Err(UsageError(message)) => Err(Failure {
            code: EXIT_USAGE,
            message,
        }),
    };
    render(json, outcome)
}

fn parse(args: &[String]) -> Result<Command, UsageError> {
    let usage = |message: &str| UsageError(message.to_owned());
    let Some((name, rest)) = args.split_first() else {
        return Ok(Command::Help);
    };
    match name.as_str() {
        "help" | "--help" | "-h" => return Ok(Command::Help),
        "--version" | "-V" => return Ok(Command::Version),
        DAEMON_HELPER_ARG => return Ok(Command::DaemonHelper),
        "list" | "current" | "switch" => {}
        other => {
            return Err(UsageError(format!(
                "Unknown command '{other}'. Run 'codexswitcher-cli help' for the commands."
            )))
        }
    }
    let mut restart = false;
    let mut no_daemon_restart = false;
    let mut plan: Option<String> = None;
    let mut selector: Option<String> = None;
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_owned())),
            _ => (arg.as_str(), None),
        };
        match flag {
            // --json is read by run(); accepted here for every command.
            // --help or -h after a command: help only. Parsing stops here, so nothing else runs.
            "--help" | "-h" if inline.is_none() => return Ok(Command::Help),
            "--json" if inline.is_none() => {}
            // Accepted for compatibility: list and current always refresh first.
            "--refresh" if name == "list" && inline.is_none() => {}
            "--restart" if name == "switch" && inline.is_none() => restart = true,
            "--no-daemon-restart" if name == "switch" && inline.is_none() => {
                no_daemon_restart = true
            }
            "--plan" if name == "list" => {
                let value = match inline {
                    Some(value) => value,
                    None => iter
                        .next()
                        .filter(|value| !value.starts_with('-'))
                        .cloned()
                        .ok_or_else(|| usage("--plan needs a value, for example --plan plus."))?,
                };
                if value.trim().is_empty() {
                    return Err(usage("--plan needs a value, for example --plan plus."));
                }
                plan = Some(value.trim().to_owned());
            }
            _ if arg.starts_with('-') => {
                return Err(UsageError(format!("Unknown option '{arg}' for '{name}'.")))
            }
            _ => {
                if name != "switch" {
                    return Err(UsageError(format!("'{name}' takes no account selector.")));
                }
                if selector.is_some() {
                    return Err(usage("Only one account selector can be given."));
                }
                selector = Some(arg.clone());
            }
        }
    }
    if restart && no_daemon_restart {
        return Err(usage("--restart and --no-daemon-restart cannot be used together."));
    }
    match name.as_str() {
        "list" => Ok(Command::List { plan }),
        "current" => Ok(Command::Current),
        _ => match selector {
            Some(selector) => Ok(Command::Switch {
                selector,
                restart,
                no_daemon_restart,
            }),
            None => Err(usage(
                "switch needs an account selector: a connection id, an email or an account id.",
            )),
        },
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
struct Failure {
    code: i32,
    message: String,
}

impl Failure {
    fn failed(message: impl Into<String>) -> Self {
        Failure {
            code: EXIT_FAILED,
            message: message.into(),
        }
    }
}

struct Success {
    /// Fields added beside `"ok": true` in JSON mode. None means text only (help).
    data: Option<Map<String, Value>>,
    text: String,
}

type Outcome = Result<Success, Failure>;

fn line(value: &Value) -> String {
    format!("{}\n", serde_json::to_string(value).unwrap_or_default())
}

fn render(json: bool, outcome: Outcome) -> Output {
    match outcome {
        Ok(success) => match (json, success.data) {
            (true, Some(mut data)) => {
                data.insert("ok".into(), Value::Bool(true));
                Output {
                    code: EXIT_OK,
                    stdout: line(&Value::Object(data)),
                    stderr: String::new(),
                }
            }
            _ => Output {
                code: EXIT_OK,
                stdout: success.text,
                stderr: String::new(),
            },
        },
        Err(failure) if json => Output {
            code: failure.code,
            stdout: line(&json!({"ok": false, "error": failure.message})),
            stderr: String::new(),
        },
        Err(failure) => Output {
            code: failure.code,
            stdout: String::new(),
            stderr: format!("codexswitcher: {}\n", failure.message),
        },
    }
}

fn object(value: Value) -> Option<Map<String, Value>> {
    match value {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn execute(command: Command) -> Outcome {
    match command {
        Command::Help => Ok(Success {
            data: None,
            text: HELP.to_owned(),
        }),
        Command::Version => {
            let version = env!("CARGO_PKG_VERSION");
            Ok(Success {
                data: object(json!({"version": version})),
                text: format!("codexswitcher {version}\n"),
            })
        }
        Command::List { plan } => list(plan.as_deref()),
        Command::Current => current(),
        Command::Switch {
            selector,
            restart,
            no_daemon_restart,
        } => switch(&selector, restart, no_daemon_restart),
        Command::DaemonHelper => {
            // Detached helper started by a switch made inside Codex. It has no output.
            if let Some(stopped) = cli::daemon_stop() {
                cli::daemon_start(stopped);
            }
            Ok(Success {
                data: None,
                text: String::new(),
            })
        }
    }
}

fn load_state() -> Result<AppState, Failure> {
    let accounts = crate::saved_accounts().map_err(|_| Failure::failed(CACHE_ERROR))?;
    Ok(AppState {
        accounts: Mutex::new(accounts),
        client: crate::http_client(),
    })
}

/// Cached accounts with `isActive` set from auth.json.
fn marked_accounts(state: &AppState) -> Result<Vec<Value>, Failure> {
    let active = auth::current().map_err(|_| Failure::failed(AUTH_ERROR))?;
    let mut accounts = state
        .accounts
        .lock()
        .map_err(|_| Failure::failed(CACHE_ERROR))?
        .clone();
    router::mark_active(&mut accounts, active.as_ref());
    Ok(accounts)
}

/// Runs the same sync as the app's button. False when it did not complete (the saved accounts
/// are then used and the caller warns); no URL, password or raw error is kept.
fn refresh_cache(state: &AppState) -> bool {
    matches!(
        tauri::async_runtime::block_on(crate::sync_accounts_core(state, None)),
        Ok(value) if value["success"] == true
    )
}

/// Warnings for a finished refresh: one generic line when the sync did not complete, otherwise
/// one line per account whose usage fetch failed (its saved values are shown instead).
fn refresh_warnings(refreshed: bool, accounts: &[Value], now: DateTime<Utc>) -> Vec<String> {
    if !refreshed {
        return vec![SYNC_WARNING.to_owned()];
    }
    accounts
        .iter()
        .filter(|account| account["usage"]["error"].is_string())
        .map(|account| {
            let who = account["email"]
                .as_str()
                .or_else(|| account["id"].as_str())
                .unwrap_or("an account");
            match usage::last_good(&account["usage"]) {
                Some(good) => format!(
                    "Could not refresh usage for {who}; showing saved values from {}.",
                    saved_time(&good["fetchedAt"], now)
                ),
                None => format!(
                    "Could not refresh usage for {who}; no saved values are available."
                ),
            }
        })
        .collect()
}

fn list(plan: Option<&str>) -> Outcome {
    let state = load_state()?;
    let refreshed = refresh_cache(&state);
    let accounts = marked_accounts(&state)?;
    if !refreshed && accounts.is_empty() {
        return Err(Failure::failed(GENERIC_SYNC_ERROR));
    }
    let now = Utc::now();
    let warnings = refresh_warnings(refreshed, &accounts, now);
    let rows = account_rows(&accounts, plan, refreshed);
    let text = format!("{}{}", warnings_text(&warnings), list_text(&rows, now));
    Ok(Success {
        data: object(json!({
            "refreshed": refreshed,
            "warnings": warnings,
            "count": rows.len(),
            "accounts": rows,
        })),
        text,
    })
}

fn current() -> Outcome {
    let state = load_state()?;
    let refreshed = refresh_cache(&state);
    let accounts = marked_accounts(&state)?;
    let Some(account) = accounts.iter().find(|account| account["isActive"] == true) else {
        return Err(Failure::failed(
            "No cached account matches the active auth.json. Run 'codexswitcher-cli list', or switch to an account.",
        ));
    };
    let now = Utc::now();
    let warnings = refresh_warnings(refreshed, std::slice::from_ref(account), now);
    let view = account_view(account, true, refreshed);
    Ok(Success {
        text: format!("{}{}", warnings_text(&warnings), account_text(&view, now)),
        data: object(json!({
            "refreshed": refreshed,
            "warnings": warnings,
            "account": view,
        })),
    })
}

/// Warnings printed before the output in text mode.
fn warnings_text(warnings: &[String]) -> String {
    warnings
        .iter()
        .map(|warning| format!("Warning: {warning}\n"))
        .collect()
}

fn switch(selector: &str, restart: bool, no_daemon_restart: bool) -> Outcome {
    let inside = calling_agent_ancestor().is_some();
    let census = cli::switch_census();
    let kind = decide_switch(restart, no_daemon_restart, census.daemon_running, inside)
        .map_err(Failure::failed)?;
    let state = load_state()?;
    let account_id = {
        let accounts = state
            .accounts
            .lock()
            .map_err(|_| Failure::failed(CACHE_ERROR))?;
        resolve_selector(&accounts, selector)?
    };

    let mut daemon = DaemonOutcome::none();
    let result = match kind {
        SwitchKind::FullRestart => {
            crate::switch_account_core(&state, &account_id, SwitchMode::Cli { restart: true })
        }
        SwitchKind::Daemon(DaemonAction::Nothing) => {
            crate::switch_account_core(&state, &account_id, SwitchMode::Cli { restart: false })
        }
        SwitchKind::Daemon(DaemonAction::RestartNow) => {
            // Stop before the auth write and start after it, even when the write fails.
            let stopped = cli::daemon_stop();
            let written =
                crate::switch_account_core(&state, &account_id, SwitchMode::Cli { restart: false });
            if let Some(stopped) = stopped {
                let (stop_ok, start_ok) = cli::daemon_start(stopped);
                daemon = DaemonOutcome {
                    action: if start_ok { "restarted" } else { "failed" },
                    stopped: Some(stop_ok),
                    started: Some(start_ok),
                };
            }
            written
        }
        SwitchKind::Daemon(DaemonAction::RestartDetached) => {
            let written =
                crate::switch_account_core(&state, &account_id, SwitchMode::Cli { restart: false });
            if written.is_ok() {
                daemon = match spawn_daemon_helper() {
                    Ok(()) => DaemonOutcome {
                        action: "scheduled",
                        stopped: None,
                        started: None,
                    },
                    Err(_) => DaemonOutcome {
                        action: "failed",
                        stopped: None,
                        started: None,
                    },
                };
            }
            written
        }
    }
    .map_err(Failure::failed)?;

    let switch_result = &result["switchResult"];
    let (email, plan, id, account_identity) = {
        let accounts = state
            .accounts
            .lock()
            .map_err(|_| Failure::failed(CACHE_ERROR))?;
        let account = accounts
            .iter()
            .find(|account| account["id"] == account_id.as_str())
            .cloned()
            .unwrap_or(Value::Null);
        (
            account["email"].clone(),
            account["plan"].clone(),
            account["id"].clone(),
            account["accountId"].clone(),
        )
    };
    let desktop_running = desktop::status()["running"] == true;
    let after = cli::switch_census();

    let mode = match kind {
        SwitchKind::FullRestart => "full-restart",
        SwitchKind::Daemon(DaemonAction::Nothing) => "auth-only",
        SwitchKind::Daemon(_) => "daemon-restart",
    };
    let cli_report = switch_result["cli"].clone();
    let mut warnings: Vec<String> = Vec::new();
    match kind {
        SwitchKind::FullRestart => {
            let failed = cli_report["failed"].as_u64().unwrap_or(0);
            if failed > 0 {
                warnings.push(format!(
                    "{failed} Codex CLI session(s) could not be restarted. Restart them by hand."
                ));
            }
        }
        _ => {
            if desktop_running {
                warnings.push("The ChatGPT desktop app is running. It was not touched; it keeps the previous account until it restarts and may overwrite auth.json.".into());
            }
            if after.embedded_sessions > 0 {
                warnings.push(format!(
                    "{} embedded Codex CLI session(s) keep the previous account. They were not touched; use --restart to close and resume them.",
                    after.embedded_sessions
                ));
            }
        }
    }
    if daemon.action == "failed" {
        warnings.push("The Codex daemon could not be restarted; daemon-connected sessions keep the previous account.".into());
    }
    if daemon.action == "scheduled" {
        warnings.push("daemon restart scheduled: finish the current turn; the existing `daemon stop` waits about 60 s for running turns and then force-stops them.".into());
        if inside {
            warnings.push("This command runs inside a Codex process. If that session is embedded or the desktop app, it keeps the previous account.".into());
        }
    }

    let mut text = format!(
        "Switched to {} (plan {}, id {}).\n",
        text_cell(&email),
        text_cell(&plan),
        text_cell(&id)
    );
    if let Some(backup) = switch_result["backupPath"].as_str() {
        text.push_str(&format!("Backup: {backup}\n"));
    }
    if let Some(steps) = switch_result["steps"].as_array() {
        for step in steps.iter().filter_map(Value::as_str) {
            text.push_str(&format!("- {step}\n"));
        }
    }
    text.push_str(&format!("Daemon: {}\n", daemon.action));
    for warning in &warnings {
        text.push_str(&format!("Warning: {warning}\n"));
    }
    let data = object(json!({
        "switched": {"id": id, "email": email, "plan": plan, "accountId": account_identity},
        "mode": mode,
        "backupPath": switch_result["backupPath"],
        "steps": switch_result["steps"],
        "daemon": daemon.to_json(),
        "running": {
            "desktopApp": desktop_running,
            "embeddedCliSessions": after.embedded_sessions,
            "daemonRunning": after.daemon_running,
        },
        "warnings": warnings,
        "cli": cli_report,
        "cacheWarning": switch_result["cacheWarning"],
    }));
    Ok(Success { data, text })
}

// ---------------------------------------------------------------------------
// Switch decisions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaemonAction {
    /// Write auth.json only.
    Nothing,
    /// Stop the daemon, write auth.json, start the daemon (synchronous).
    RestartNow,
    /// Write auth.json, then a detached helper restarts the daemon after this command returns.
    RestartDetached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwitchKind {
    /// `--restart`: the full GUI-like flow.
    FullRestart,
    /// Default or `--no-daemon-restart`.
    Daemon(DaemonAction),
}

/// Pure decision for a switch. `--restart` is refused inside Codex. Without `--restart`, the
/// daemon is restarted only when it runs and `--no-daemon-restart` is absent; inside Codex the
/// restart is detached because a synchronous stop would wait for the running turn.
fn decide_switch(
    restart: bool,
    no_daemon_restart: bool,
    daemon_running: bool,
    inside_codex: bool,
) -> Result<SwitchKind, &'static str> {
    if restart {
        return if inside_codex {
            Err(REFUSE_RESTART)
        } else {
            Ok(SwitchKind::FullRestart)
        };
    }
    let action = if no_daemon_restart || !daemon_running {
        DaemonAction::Nothing
    } else if inside_codex {
        DaemonAction::RestartDetached
    } else {
        DaemonAction::RestartNow
    };
    Ok(SwitchKind::Daemon(action))
}

struct DaemonOutcome {
    action: &'static str,
    stopped: Option<bool>,
    started: Option<bool>,
}

impl DaemonOutcome {
    fn none() -> Self {
        DaemonOutcome {
            action: "none",
            stopped: None,
            started: None,
        }
    }

    fn to_json(&self) -> Value {
        json!({"action": self.action, "stopped": self.stopped, "started": self.started})
    }
}

/// Matches the selector against connection id, account id and email (case-insensitive).
/// Exactly one account must match.
fn resolve_selector(accounts: &[Value], selector: &str) -> Result<String, Failure> {
    let matches: Vec<&Value> = accounts
        .iter()
        .filter(|account| matches_selector(account, selector))
        .collect();
    match matches.as_slice() {
        [] => Err(Failure::failed(format!(
            "No cached account matches '{selector}'. Run 'codexswitcher-cli list' to see the selectors."
        ))),
        [one] => one["id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Failure::failed("The matching account has no id in the cache.")),
        many => {
            let ids: Vec<String> = many
                .iter()
                .map(|account| {
                    format!(
                        "{} ({})",
                        account["id"].as_str().unwrap_or("?"),
                        account["email"].as_str().unwrap_or("?")
                    )
                })
                .collect();
            Err(Failure::failed(format!(
                "'{selector}' matches {} accounts. Use one of these ids: {}.",
                many.len(),
                ids.join(", ")
            )))
        }
    }
}

fn matches_selector(account: &Value, selector: &str) -> bool {
    account["id"].as_str() == Some(selector)
        || account["accountId"].as_str() == Some(selector)
        || account["email"]
            .as_str()
            .is_some_and(|email| email.eq_ignore_ascii_case(selector))
}

// ---------------------------------------------------------------------------
// Account views (only allow-listed fields leave this module)
// ---------------------------------------------------------------------------

fn account_rows(accounts: &[Value], plan: Option<&str>, refreshed: bool) -> Vec<Value> {
    accounts
        .iter()
        .filter(|account| {
            plan.is_none_or(|wanted| {
                account["plan"]
                    .as_str()
                    .is_some_and(|value| value.eq_ignore_ascii_case(wanted))
            })
        })
        .map(|account| account_view(account, account["isActive"] == true, refreshed))
        .collect()
}

fn account_view(account: &Value, active: bool, refreshed: bool) -> Value {
    json!({
        "id": account["id"],
        "email": account["email"],
        "plan": account["plan"],
        "accountId": account["accountId"],
        "active": active,
        "expiresAt": account["expiresAt"],
        "usage": usage_view(&account["usage"], refreshed),
    })
}

/// Numbers to show: this sync's values when they are fresh, otherwise the last good values.
/// `stale` is true unless this run fetched them without error.
fn usage_view(usage: &Value, refreshed: bool) -> Value {
    if !usage.is_object() {
        return Value::Null;
    }
    // Only the fixed status text is shown; other error text is replaced.
    let error = usage["error"].as_str().map(|text| {
        if text.starts_with("Status ") {
            text.to_owned()
        } else {
            "Usage could not be fetched.".to_owned()
        }
    });
    let none = Value::Null;
    let shown = usage::last_good(usage).unwrap_or(&none);
    json!({
        "fiveHourRemainingPercent": remaining_percent(&shown["primary"]),
        "fiveHourResetAt": reset_at(&shown["primary"]["resetAt"]),
        "weeklyRemainingPercent": remaining_percent(&shown["secondary"]),
        "weeklyResetAt": reset_at(&shown["secondary"]["resetAt"]),
        "limitReached": shown["limitReached"].as_bool(),
        "resetCredits": shown["resetCredits"].as_u64(),
        "fetchedAt": shown["fetchedAt"].as_str(),
        "error": error,
        "stale": !(refreshed && usage["error"].is_null()),
    })
}

/// True when a usage view has any number to show.
fn has_values(usage: &Value) -> bool {
    !usage["fiveHourRemainingPercent"].is_null() || !usage["weeklyRemainingPercent"].is_null()
}

/// Local time of saved values with their approximate age, or "unknown time".
fn saved_time(fetched_at: &Value, now: DateTime<Utc>) -> String {
    let Some(time) = fetched_at
        .as_str()
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|time| time.with_timezone(&Utc))
    else {
        return "unknown time".into();
    };
    format!(
        "{} ({})",
        time.with_timezone(&Local).format("%Y-%m-%d %H:%M"),
        age_text((now - time).num_minutes())
    )
}

/// Approximate age in minutes, as "N min ago", "N h ago" or "N days ago".
fn age_text(minutes: i64) -> String {
    let minutes = minutes.max(0);
    if minutes < 60 {
        format!("{minutes} min ago")
    } else if minutes < 48 * 60 {
        format!("{} h ago", minutes / 60)
    } else {
        format!("{} days ago", minutes / (24 * 60))
    }
}

/// Share of a usage window still available, 0 to 100.
fn remaining_percent(window: &Value) -> Option<i64> {
    window["usedPercent"]
        .as_f64()
        .map(|used| (100.0 - used).clamp(0.0, 100.0).round() as i64)
}

/// Reset time as RFC 3339. Accepts unix seconds or an existing string.
fn reset_at(value: &Value) -> Option<String> {
    match value {
        Value::Number(_) => value
            .as_i64()
            .and_then(|secs| DateTime::<Utc>::from_timestamp(secs, 0))
            .map(|time| time.to_rfc3339()),
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Text rendering
// ---------------------------------------------------------------------------

fn text_cell(value: &Value) -> String {
    match value {
        Value::Null => "-".into(),
        Value::String(text) if text.is_empty() => "-".into(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn percent_cell(value: &Value) -> String {
    value.as_i64().map_or("-".into(), |percent| format!("{percent}%"))
}

/// Local time for an RFC 3339 string; the raw text when it does not parse.
fn local_time(value: &Value) -> String {
    match value.as_str() {
        Some(text) => DateTime::parse_from_rfc3339(text)
            .map(|time| time.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|_| text.to_owned()),
        None => "-".into(),
    }
}

fn window_cell(percent: &Value, reset: &Value) -> String {
    format!("{} (resets {})", percent_cell(percent), local_time(reset))
}

/// The FETCHED cell: the fetch time, or for saved values "saved <time> (<age>)".
fn fetched_cell(usage: &Value, now: DateTime<Utc>) -> String {
    if usage["stale"] != true {
        return local_time(&usage["fetchedAt"]);
    }
    if !has_values(usage) {
        return "-".into();
    }
    format!("saved {}", saved_time(&usage["fetchedAt"], now))
}

fn list_text(rows: &[Value], now: DateTime<Utc>) -> String {
    if rows.is_empty() {
        return "No accounts in the cache. Sync from the app, or run 'codexswitcher-cli list'.\n"
            .to_owned();
    }
    let headers = [
        "", "ID", "EMAIL", "PLAN", "5H LEFT", "WEEK LEFT", "5H RESETS", "WEEK RESETS", "CREDITS",
        "EXPIRES", "FETCHED",
    ];
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            let usage = &row["usage"];
            vec![
                if row["active"] == true { "*".into() } else { String::new() },
                text_cell(&row["id"]),
                text_cell(&row["email"]),
                text_cell(&row["plan"]),
                percent_cell(&usage["fiveHourRemainingPercent"]),
                percent_cell(&usage["weeklyRemainingPercent"]),
                local_time(&usage["fiveHourResetAt"]),
                local_time(&usage["weeklyResetAt"]),
                text_cell(&usage["resetCredits"]),
                local_time(&row["expiresAt"]),
                fetched_cell(usage, now),
            ]
        })
        .collect();
    let mut out = table(&headers, &body);
    for row in rows {
        if let Some(error) = row["usage"]["error"].as_str() {
            out.push_str(&format!(
                "note: {}: usage error: {error}\n",
                text_cell(&row["id"])
            ));
        }
    }
    out
}

fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.chars().count()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }
    let render_row = |cells: &[String]| -> String {
        let padded: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| format!("{cell:<width$}", width = widths[index]))
            .collect();
        format!("{}\n", padded.join("  ").trim_end())
    };
    let header_cells: Vec<String> = headers.iter().map(|header| header.to_string()).collect();
    let mut out = render_row(&header_cells);
    for row in rows {
        out.push_str(&render_row(row));
    }
    out
}

fn account_text(view: &Value, now: DateTime<Utc>) -> String {
    let usage = &view["usage"];
    let mut rows: Vec<(&str, String)> = vec![
        ("Email", text_cell(&view["email"])),
        ("ID", text_cell(&view["id"])),
        ("Account id", text_cell(&view["accountId"])),
        ("Plan", text_cell(&view["plan"])),
        ("Expires", local_time(&view["expiresAt"])),
        (
            "5-hour left",
            window_cell(&usage["fiveHourRemainingPercent"], &usage["fiveHourResetAt"]),
        ),
        (
            "Weekly left",
            window_cell(&usage["weeklyRemainingPercent"], &usage["weeklyResetAt"]),
        ),
        ("Credits", text_cell(&usage["resetCredits"])),
        ("Fetched", fetched_cell(usage, now)),
    ];
    if let Some(error) = usage["error"].as_str() {
        rows.push(("Usage error", error.to_owned()));
    }
    rows.iter()
        .map(|(label, value)| format!("{label:<13}{value}\n"))
        .collect()
}

// ---------------------------------------------------------------------------
// Process checks and the detached helper
// ---------------------------------------------------------------------------

/// True when a process is a Codex CLI / app-server or the ChatGPT/Codex desktop app, or a
/// node/bun process running the Codex CLI script. Restarting would close such a process.
pub(crate) fn blocks_restart(name: &str, args: &[String]) -> bool {
    let stem = executable_stem(name);
    if stem == "codex" || stem.starts_with("codex-") || desktop::is_desktop_process(name) {
        return true;
    }
    if stem == "node" || stem == "bun" {
        return args
            .iter()
            .any(|arg| script_stem(arg) == "codex");
    }
    false
}

fn executable_stem(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_owned()
}

/// File name without directories and without its last extension, lowercase.
fn script_stem(arg: &str) -> String {
    let segment = arg.rsplit(['\\', '/']).next().unwrap_or("");
    let stem = segment.rsplit_once('.').map_or(segment, |(stem, _)| stem);
    stem.to_ascii_lowercase()
}

/// Name of the nearest ancestor process that is a Codex or ChatGPT process, if any.
fn calling_agent_ancestor() -> Option<String> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let mut current = std::process::id();
    let mut seen: HashSet<u32> = HashSet::new();
    for _ in 0..MAX_ANCESTORS {
        if !seen.insert(current) {
            break; // pid cycle
        }
        let Some(parent) = system
            .process(Pid::from_u32(current))
            .and_then(|process| process.parent())
        else {
            break;
        };
        let Some(process) = system.process(parent) else {
            break;
        };
        let name = process.name().to_string_lossy().into_owned();
        let args: Vec<String> = process
            .cmd()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        if blocks_restart(&name, &args) {
            return Some(name);
        }
        current = parent.as_u32();
    }
    None
}

fn spawn_daemon_helper() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    spawn_detached(&exe, DAEMON_HELPER_ARG)
}

#[cfg(windows)]
fn spawn_detached(exe: &std::path::Path, arg: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
    let attempt = |flags: u32| {
        Command::new(exe)
            .arg(arg)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    // Breakaway lets the helper outlive a job that would kill it; some jobs refuse it.
    match attempt(base | CREATE_BREAKAWAY_FROM_JOB) {
        Ok(_) => Ok(()),
        Err(_) => attempt(base).map(|_| ()).map_err(|error| error.to_string()),
    }
}

#[cfg(not(windows))]
fn spawn_detached(exe: &std::path::Path, arg: &str) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    Command::new(exe)
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// Console handles
// ---------------------------------------------------------------------------

mod sinks {
    use super::Output;
    use std::io::Write;

    #[cfg(windows)]
    use std::ffi::c_void;

    #[cfg(windows)]
    #[derive(Clone, Copy)]
    enum Sink {
        /// A pipe or file handle owned by the caller. Written to as is, never closed.
        Redirected(usize),
        /// A console: written through CONOUT$ after attaching the parent console.
        Console,
    }

    #[cfg(windows)]
    pub struct Sinks {
        stdout: Sink,
        stderr: Sink,
    }

    #[cfg(windows)]
    impl Sinks {
        pub fn capture() -> Self {
            use windows::Win32::System::Console::{STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};
            Sinks {
                stdout: classify(STD_OUTPUT_HANDLE),
                stderr: classify(STD_ERROR_HANDLE),
            }
        }

        pub fn write(&self, output: &Output) {
            write_sink(self.stdout, output.stdout.as_bytes());
            write_sink(self.stderr, output.stderr.as_bytes());
        }
    }

    #[cfg(windows)]
    fn classify(id: windows::Win32::System::Console::STD_HANDLE) -> Sink {
        use windows::Win32::Foundation::{HANDLE_FLAG_INHERIT, HANDLE_FLAGS};
        use windows::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_CHAR, FILE_TYPE_UNKNOWN};
        use windows::Win32::System::Console::GetStdHandle;
        // A windowed (windows_subsystem = "windows") exe started from an interactive
        // shell has no std handles. A missing, null or invalid handle means "try the
        // parent console", as does an unknown file type.
        let Ok(handle) = (unsafe { GetStdHandle(id) }) else {
            return Sink::Console;
        };
        if handle.is_invalid() || handle.0.is_null() {
            return Sink::Console;
        }
        let file_type = unsafe { GetFileType(handle) };
        if file_type == FILE_TYPE_CHAR || file_type == FILE_TYPE_UNKNOWN {
            return Sink::Console;
        }
        // Children spawned later (daemon commands, the helper) must not inherit the pipe,
        // or the caller would wait for end-of-output until they exit.
        unsafe {
            let _ = windows::Win32::Foundation::SetHandleInformation(
                handle,
                HANDLE_FLAG_INHERIT.0,
                HANDLE_FLAGS(0),
            );
        }
        Sink::Redirected(handle.0 as usize)
    }

    #[cfg(windows)]
    fn write_sink(sink: Sink, bytes: &[u8]) {
        use std::os::windows::io::FromRawHandle;
        if bytes.is_empty() {
            return;
        }
        match sink {
            Sink::Redirected(raw) => {
                // The handle belongs to the caller: wrap it without closing it.
                let mut file = std::mem::ManuallyDrop::new(unsafe {
                    std::fs::File::from_raw_handle(raw as *mut c_void)
                });
                let _ = file.write_all(bytes);
            }
            // Kept as is on purpose: `switch --restart` closes and reattaches consoles, so the
            // output must go to the parent console through CONOUT$ after that, not to the
            // handle captured at startup.
            Sink::Console => {
                use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
                unsafe {
                    let _ = AttachConsole(ATTACH_PARENT_PROCESS);
                }
                if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
                    let _ = file.write_all(bytes);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod sinks {
    use super::Output;
    use std::io::Write;

    pub struct Sinks;

    impl Sinks {
        pub fn capture() -> Self {
            Sinks
        }

        pub fn write(&self, output: &Output) {
            let _ = std::io::stdout().write_all(output.stdout.as_bytes());
            let _ = std::io::stderr().write_all(output.stderr.as_bytes());
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| item.to_string()).collect()
    }

    fn account(id: &str, email: &str, plan: &str, account_id: &str) -> Value {
        json!({
            "id": id, "email": email, "plan": plan, "accountId": account_id,
            "expiresAt": "2026-12-01T00:00:00Z", "isActive": false,
            "tokens": {"id_token": "tok-id-secret", "access_token": "tok-access-secret", "refresh_token": "tok-refresh-secret", "account_id": account_id},
            "apiKey": "sk-api-secret",
            "usage": {"primary": {"usedPercent": 20.0, "resetAt": 1795000000}, "secondary": {"usedPercent": 55.4, "resetAt": 1795600000}, "limitReached": false, "resetCredits": 2, "fetchedAt": "2026-11-26T10:00:00Z"},
            "providerName": "9Router Codex"
        })
    }

    #[test]
    fn parses_list_options() {
        assert_eq!(
            parse(&args(&["list", "--plan", "plus", "--json"])),
            Ok(Command::List { plan: Some("plus".into()) })
        );
        assert_eq!(
            parse(&args(&["list", "--plan=Pro", "--refresh"])),
            Ok(Command::List { plan: Some("Pro".into()) })
        );
        assert!(parse(&args(&["list", "--plan"])).is_err());
        assert!(parse(&args(&["list", "--plan", "--json"])).is_err());
        assert!(parse(&args(&["list", "--restart"])).is_err());
        assert!(parse(&args(&["list", "extra"])).is_err());
    }

    #[test]
    fn parses_switch_options() {
        assert_eq!(
            parse(&args(&["switch", "a@example.test"])),
            Ok(Command::Switch { selector: "a@example.test".into(), restart: false, no_daemon_restart: false })
        );
        assert_eq!(
            parse(&args(&["switch", "acc", "--no-daemon-restart", "--json"])),
            Ok(Command::Switch { selector: "acc".into(), restart: false, no_daemon_restart: true })
        );
        assert!(parse(&args(&["switch"])).is_err());
        assert!(parse(&args(&["switch", "a", "b"])).is_err());
        assert!(parse(&args(&["switch", "a", "--restart", "--no-daemon-restart"])).is_err());
        assert!(parse(&args(&["switch", "a", "--refresh"])).is_err());
    }

    #[test]
    fn help_after_a_command_prints_help_and_never_switches() {
        // Parsing stops at --help, so no later argument (such as a selector) can run a switch.
        assert_eq!(parse(&args(&["switch", "acc", "--help"])), Ok(Command::Help));
        assert_eq!(parse(&args(&["switch", "--restart", "-h"])), Ok(Command::Help));
        assert_eq!(parse(&args(&["list", "--json", "--help"])), Ok(Command::Help));
        let out = render_request(&args(&["switch", "acc", "--help"]));
        assert_eq!((out.code, out.stdout.as_str()), (EXIT_OK, HELP));
        // --help=value is not the flag.
        assert!(parse(&args(&["switch", "acc", "--help=yes"])).is_err());
    }
    #[test]
    fn parses_help_version_and_unknown_commands() {
        assert_eq!(parse(&args(&["-h"])), Ok(Command::Help));
        assert_eq!(parse(&args(&["--version"])), Ok(Command::Version));
        assert_eq!(parse(&args(&["current"])), Ok(Command::Current));
        assert!(parse(&args(&["lsit"])).is_err());
        assert!(parse(&args(&["current", "--bogus"])).is_err());
    }

    #[test]
    fn dash_arguments_keep_gui_startup() {
        assert!(!is_cli_request(&args(&[])));
        assert!(!is_cli_request(&args(&["--hidden"])));
        assert!(!is_cli_request(&args(&["--uninstall-cleanup"])));
        assert!(!is_cli_request(&args(&["--uninstall-cleanup-silent"])));
        assert!(!is_cli_request(&args(&["-x"])));
        assert!(is_cli_request(&args(&["--help"])));
        assert!(is_cli_request(&args(&["-V"])));
        assert!(is_cli_request(&args(&["lsit"])));
    }

    #[test]
    fn cli_routes_every_argument_list_through_the_parser() {
        // Only parsing and pure commands run here: never switch or list --refresh.
        let none = render_request(&args(&[]));
        assert_eq!(none.code, EXIT_OK);
        assert!(none.stdout.starts_with("CodexSwitcher command line"));
        assert_eq!(render_request(&args(&["--help"])).code, EXIT_OK);
        assert_eq!(render_request(&args(&["-V"])).code, EXIT_OK);
        // GUI-only and unknown flags are usage errors in the console binary.
        for flag in ["--uninstall-cleanup", "--uninstall-cleanup-silent", "--hidden", "-x"] {
            let output = render_request(&args(&[flag]));
            assert_eq!(output.code, EXIT_USAGE, "{flag}");
            assert!(output.stdout.is_empty(), "{flag}");
        }
        assert_eq!(render_request(&args(&["lsit"])).code, EXIT_USAGE);
        // The internal daemon-restart argument is accepted by the console binary too.
        assert_eq!(parse(&args(&[DAEMON_HELPER_ARG])), Ok(Command::DaemonHelper));
    }

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-26T12:00:00Z").unwrap().with_timezone(&Utc)
    }

    fn good_usage(fetched_at: &str) -> Value {
        json!({
            "primary": {"usedPercent": 20.0, "resetAt": 1795000000},
            "secondary": {"usedPercent": 55.0, "resetAt": 1795600000},
            "limitReached": false, "resetCredits": 2, "fetchedAt": fetched_at
        })
    }

    #[test]
    fn stale_usage_shows_last_good_numbers_and_their_time() {
        let usage = json!({"error": "Status 500", "lastGood": good_usage("2026-11-26T09:00:00+00:00")});
        let stale = usage_view(&usage, true);
        assert_eq!(stale["stale"], true);
        assert_eq!(stale["error"], "Status 500");
        assert_eq!(stale["fiveHourRemainingPercent"], 80);
        assert_eq!(stale["weeklyRemainingPercent"], 45);
        assert_eq!(stale["fetchedAt"], "2026-11-26T09:00:00+00:00");
        // Saved values with no error this run are stale too; the error stays null.
        let saved = usage_view(&good_usage("2026-11-26T09:00:00+00:00"), false);
        assert_eq!((saved["stale"].clone(), saved["error"].clone()), (json!(true), Value::Null));
        assert_eq!(saved["fiveHourRemainingPercent"], 80);
        // Fresh values from this run.
        let fresh = usage_view(&good_usage("2026-11-26T11:59:00+00:00"), true);
        assert_eq!(fresh["stale"], false);
        // A failed fetch without earlier values has no numbers.
        let none = usage_view(&json!({"error": "Status 401"}), true);
        assert_eq!((none["stale"].clone(), none["fiveHourRemainingPercent"].clone()), (json!(true), Value::Null));
        assert_eq!(none["fetchedAt"], Value::Null);
    }

    #[test]
    fn refresh_warnings_name_the_account_and_the_saved_time() {
        let now = fixed_now();
        assert_eq!(refresh_warnings(false, &[], now), vec![SYNC_WARNING.to_owned()]);
        let mut failed = account("conn-1", "a@example.invalid", "plus", "acc-1");
        failed["usage"] = json!({"error": "Status 500", "lastGood": good_usage("2026-11-26T09:00:00+00:00")});
        let mut no_values = account("conn-2", "b@example.invalid", "plus", "acc-2");
        no_values["usage"] = json!({"error": "Status 401"});
        let fine = account("conn-3", "c@example.invalid", "plus", "acc-3");
        let warnings = refresh_warnings(true, &[failed, no_values, fine], now);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].starts_with("Could not refresh usage for a@example.invalid; showing saved values from 2026-11-26"));
        assert!(warnings[0].ends_with("(3 h ago)."));
        assert_eq!(warnings[1], "Could not refresh usage for b@example.invalid; no saved values are available.");
    }

    #[test]
    fn age_and_unknown_saved_time() {
        assert_eq!(age_text(0), "0 min ago");
        assert_eq!(age_text(59), "59 min ago");
        assert_eq!(age_text(60), "1 h ago");
        assert_eq!(age_text(47 * 60), "47 h ago");
        assert_eq!(age_text(48 * 60), "2 days ago");
        assert_eq!(saved_time(&Value::Null, fixed_now()), "unknown time");
        assert_eq!(warnings_text(&["x".into()]), "Warning: x\n");
    }

    #[test]
    fn stale_rows_show_saved_values_with_time_in_text() {
        let now = fixed_now();
        let mut stale = account("conn-1", "a@example.invalid", "plus", "acc-1");
        stale["usage"] = json!({"error": "Status 500", "lastGood": good_usage("2026-11-26T09:00:00+00:00")});
        let mut empty = account("conn-2", "b@example.invalid", "free", "acc-2");
        empty["usage"] = json!({"error": "Status 401"});
        let rows = account_rows(&[stale, empty], None, true);
        let text = list_text(&rows, now);
        assert!(text.contains("saved "), "{text}");
        assert!(text.contains("(3 h ago)"), "{text}");
        // A row without saved numbers shows no numbers and no time.
        let empty_line = text.lines().find(|line| line.contains("b@example.invalid")).unwrap();
        assert!(!empty_line.contains("saved "));
        assert!(empty_line.ends_with('-'), "{empty_line}");
        assert!(text.contains("usage error: Status 401"), "{text}");
        // Whole-sync failure: the saved values of every row are marked stale.
        let rows = account_rows(&[account("conn-3", "c@example.invalid", "plus", "acc-3")], None, false);
        assert_eq!(rows[0]["usage"]["stale"], true);
        assert_eq!(rows[0]["usage"]["fiveHourRemainingPercent"], 80);
    }

    #[test]
    fn switch_decision_table() {
        // --restart inside Codex is refused; outside it is the full flow.
        assert_eq!(decide_switch(true, false, true, true), Err(REFUSE_RESTART));
        assert_eq!(decide_switch(true, false, false, false), Ok(SwitchKind::FullRestart));
        // Default: nothing to do without a daemon.
        assert_eq!(decide_switch(false, false, false, false), Ok(SwitchKind::Daemon(DaemonAction::Nothing)));
        assert_eq!(decide_switch(false, false, false, true), Ok(SwitchKind::Daemon(DaemonAction::Nothing)));
        // Default with daemon: synchronous outside Codex, detached inside.
        assert_eq!(decide_switch(false, false, true, false), Ok(SwitchKind::Daemon(DaemonAction::RestartNow)));
        assert_eq!(decide_switch(false, false, true, true), Ok(SwitchKind::Daemon(DaemonAction::RestartDetached)));
        // --no-daemon-restart never touches the daemon.
        assert_eq!(decide_switch(false, true, true, false), Ok(SwitchKind::Daemon(DaemonAction::Nothing)));
        assert_eq!(decide_switch(false, true, true, true), Ok(SwitchKind::Daemon(DaemonAction::Nothing)));
    }

    #[test]
    fn detects_codex_and_desktop_ancestors() {
        let none: Vec<String> = Vec::new();
        assert!(blocks_restart("codex.exe", &none));
        assert!(blocks_restart("Codex.exe", &none));
        assert!(blocks_restart("codex-app-server.exe", &none));
        assert!(blocks_restart("ChatGPT.exe", &none));
        assert!(!blocks_restart("codexswitcher.exe", &none));
        assert!(!blocks_restart("CodexSwitcher", &none));
        // The console command line is never a Codex or ChatGPT process.
        assert!(!blocks_restart("codexswitcher-cli.exe", &none));
        assert!(!blocks_restart("CodexSwitcher-CLI", &none));
        assert!(!blocks_restart("explorer.exe", &none));
        assert!(!blocks_restart("powershell.exe", &none));
        let codex_script = args(&["C:\\Program Files\\nodejs\\node.exe", "C:\\npm\\node_modules\\@openai\\codex\\bin\\codex.js"]);
        assert!(blocks_restart("node.exe", &codex_script));
        assert!(!blocks_restart("node.exe", &args(&["server.js"])));
    }

    #[test]
    fn selector_must_match_exactly_one_account() {
        let accounts = vec![
            account("conn-1", "Person@Example.test", "plus", "acc-1"),
            account("conn-2", "other@example.test", "pro", "acc-2"),
            account("conn-3", "third@example.test", "pro", "acc-2"),
        ];
        assert_eq!(resolve_selector(&accounts, "conn-1").unwrap(), "conn-1");
        assert_eq!(resolve_selector(&accounts, "person@example.TEST").unwrap(), "conn-1");
        let shared = resolve_selector(&accounts, "acc-2").unwrap_err();
        assert!(shared.message.contains("conn-2"));
        assert!(shared.message.contains("conn-3"));
        assert_eq!(resolve_selector(&accounts, "missing").unwrap_err().code, EXIT_FAILED);
    }

    #[test]
    fn remaining_percent_and_reset_times() {
        assert_eq!(remaining_percent(&json!({"usedPercent": 55.4})), Some(45));
        assert_eq!(remaining_percent(&json!({"usedPercent": 130.0})), Some(0));
        assert_eq!(remaining_percent(&json!({})), None);
        assert!(reset_at(&json!(1795000000)).unwrap().starts_with("2026-"));
        assert_eq!(reset_at(&json!("2026-12-01T00:00:00Z")).as_deref(), Some("2026-12-01T00:00:00Z"));
        assert_eq!(reset_at(&Value::Null), None);
    }

    #[test]
    fn list_and_current_output_contains_no_secrets() {
        let mut accounts = vec![
            account("conn-1", "a@example.test", "plus", "acc-1"),
            account("conn-2", "b@example.test", "free", "acc-2"),
        ];
        accounts[0]["usage"] = json!({"error": "https://router.example.test password=router-password-secret"});
        router::mark_active(&mut accounts, Some(&json!({"accountId": "acc-1", "email": "a@example.test", "tokens": {}})));
        let now = Utc::now();
        let rows = account_rows(&accounts, None, true);
        let json_text = line(&json!({"ok": true, "count": rows.len(), "accounts": rows}));
        let text = list_text(&rows, now);
        let current_text = account_text(&account_view(&accounts[0], true, true), now);
        for output in [&json_text, &text, &current_text] {
            for secret in [
                "tok-id-secret", "tok-access-secret", "tok-refresh-secret", "sk-api-secret",
                "router.example.test", "router-password-secret", "\"tokens\"", "apiKey",
            ] {
                assert!(!output.contains(secret), "output leaked {secret}");
            }
        }
        // The error text from the sync path is replaced by a fixed message.
        assert!(text.contains("Usage could not be fetched."));
        // Every JSON account field comes from the allow list.
        let allowed = ["id", "email", "plan", "accountId", "active", "expiresAt", "usage"];
        for row in rows.iter() {
            for key in row.as_object().unwrap().keys() {
                assert!(allowed.contains(&key.as_str()), "unexpected field {key}");
            }
        }
        assert!(!GENERIC_SYNC_ERROR.contains("http"));
    }

    #[test]
    fn plan_filter_is_case_insensitive() {
        let accounts = vec![account("conn-1", "a@example.test", "plus", "acc-1"), account("conn-2", "b@example.test", "free", "acc-2")];
        let rows = account_rows(&accounts, Some("PLUS"), true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "conn-1");
    }

    #[test]
    fn json_failure_and_help_rendering() {
        let failed = render(true, Err(Failure::failed("No match.")));
        assert_eq!(failed.code, EXIT_FAILED);
        assert_eq!(failed.stdout, "{\"error\":\"No match.\",\"ok\":false}\n");
        let help = render(true, execute(Command::Help));
        assert!(help.stdout.starts_with("CodexSwitcher command line"));
        assert!(!help.stdout.contains("--internal-daemon-restart"));
        let text_failure = render(false, Err(Failure::failed("Nope.")));
        assert!(text_failure.stdout.is_empty());
        assert_eq!(text_failure.stderr, "codexswitcher: Nope.\n");
    }
}
