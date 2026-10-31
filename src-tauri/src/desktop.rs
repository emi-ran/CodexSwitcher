use serde_json::{json, Value};
use std::process::Command;
#[cfg(not(windows))]
use sysinfo::Signal;
use sysinfo::{ProcessesToUpdate, System};

pub(crate) fn is_desktop_process(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    #[cfg(windows)]
    {
        name == "chatgpt.exe"
    }
    #[cfg(not(windows))]
    {
        name == "chatgpt"
    }
}

pub fn status() -> Value {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let processes: Vec<Value> = system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            let name = process.name().to_string_lossy();
            if is_desktop_process(&name) {
                Some(json!({"name": name, "pid": pid.to_string()}))
            } else {
                None
            }
        })
        .collect();
    json!({"running": !processes.is_empty(), "processes": processes})
}

pub fn stop() -> Result<Value, String> {
    stop_internal(
        || status()["running"].as_bool().unwrap_or(false),
        || {
            let mut system = System::new();
            system.refresh_processes(ProcessesToUpdate::All, true);
            for process in system.processes().values() {
                if is_desktop_process(&process.name().to_string_lossy()) {
                    #[cfg(windows)]
                    let _ = process.kill();
                    #[cfg(not(windows))]
                    let _ = process
                        .kill_with(Signal::Term)
                        .unwrap_or_else(|| process.kill());
                }
            }
        },
        50,
        std::time::Duration::from_millis(100),
    )
}

pub(crate) fn stop_internal<S, K>(
    mut is_running: S,
    mut kill_all: K,
    max_attempts: usize,
    delay: std::time::Duration,
) -> Result<Value, String>
where
    S: FnMut() -> bool,
    K: FnMut(),
{
    // If already closed, exit immediately without error.
    if !is_running() {
        return Ok(json!({"success": true}));
    }

    kill_all();

    for attempt in 0..max_attempts {
        if !is_running() {
            return Ok(json!({"success": true}));
        }
        // Retry killing once after 1.5s if stubborn processes remain
        if attempt == 15 {
            kill_all();
        }
        std::thread::sleep(delay);
    }

    if !is_running() {
        return Ok(json!({"success": true}));
    }

    Err("ChatGPT desktop app did not exit; auth.json was not changed".into())
}

pub fn start() -> Result<Value, String> {
    #[cfg(windows)]
    let mut command = {
        let mut value = Command::new("explorer.exe");
        value.arg("shell:AppsFolder\\OpenAI.Codex_2p2nqsd0c76g0!App");
        value
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut value = Command::new("open");
        value.args(["-a", "ChatGPT"]);
        value
    };
    #[cfg(target_os = "linux")]
    let mut command = Command::new("chatgpt");
    command
        .spawn()
        .map_err(|e| format!("Could not launch ChatGPT desktop app: {e}"))?;
    Ok(json!({"success": true, "message": "ChatGPT desktop app launch requested"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn process_name_detection_matches_chatgpt() {
        #[cfg(windows)]
        {
            assert!(is_desktop_process("ChatGPT.exe"));
            assert!(is_desktop_process("chatgpt.exe"));
            assert!(is_desktop_process("CHATGPT.EXE"));
            assert!(!is_desktop_process("codexswitcher.exe"));
            assert!(!is_desktop_process("cmd.exe"));
        }
        #[cfg(not(windows))]
        {
            assert!(is_desktop_process("chatgpt"));
            assert!(is_desktop_process("ChatGPT"));
            assert!(!is_desktop_process("codexswitcher"));
        }
    }

    #[test]
    fn stop_returns_ok_immediately_when_already_closed() {
        let killed = Arc::new(AtomicBool::new(false));
        let killed_clone = killed.clone();

        let result = stop_internal(
            || false,
            move || killed_clone.store(true, Ordering::SeqCst),
            10,
            std::time::Duration::from_millis(1),
        );

        assert_eq!(result.unwrap(), json!({"success": true}));
        assert!(!killed.load(Ordering::SeqCst));
    }

    #[test]
    fn stop_succeeds_when_processes_exit_after_kill() {
        let runs = Arc::new(AtomicUsize::new(0));
        let runs_clone = runs.clone();

        let result = stop_internal(
            move || {
                let current = runs_clone.fetch_add(1, Ordering::SeqCst);
                current == 0
            },
            || {},
            10,
            std::time::Duration::from_millis(1),
        );

        assert_eq!(result.unwrap(), json!({"success": true}));
    }

    #[test]
    fn stop_fails_safely_when_process_refuses_to_exit() {
        let kill_count = Arc::new(AtomicUsize::new(0));
        let kill_count_clone = kill_count.clone();

        let result = stop_internal(
            || true,
            move || {
                kill_count_clone.fetch_add(1, Ordering::SeqCst);
            },
            20,
            std::time::Duration::from_millis(1),
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "ChatGPT desktop app did not exit; auth.json was not changed"
        );
        assert_eq!(kill_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn reproduces_bug_where_kill_failure_on_exited_process_causes_false_error() {
        let old_stop_logic = |kill_returns: bool, is_actually_running: bool| -> Result<Value, String> {
            if !kill_returns {
                return Err("Could not stop ChatGPT.exe".into());
            }
            if !is_actually_running {
                Ok(json!({"success": true}))
            } else {
                Err("ChatGPT desktop app did not exit; auth.json was not changed".into())
            }
        };

        // Old logic falsely reported error when kill returned false even though process was closed:
        assert_eq!(
            old_stop_logic(false, false).unwrap_err(),
            "Could not stop ChatGPT.exe"
        );

        // Fixed logic checks actual running state and succeeds without error:
        let fixed_result = stop_internal(
            || false,
            || {},
            10,
            std::time::Duration::from_millis(1),
        );
        assert_eq!(fixed_result.unwrap(), json!({"success": true}));
    }
}
