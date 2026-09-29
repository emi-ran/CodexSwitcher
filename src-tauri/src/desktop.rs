use serde_json::{json, Value};
use std::process::Command;
#[cfg(not(windows))]
use sysinfo::Signal;
use sysinfo::{ProcessesToUpdate, System};

fn is_desktop_process(name: &str) -> bool {
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
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    for process in system.processes().values() {
        if is_desktop_process(&process.name().to_string_lossy()) {
            #[cfg(windows)]
            let stopped = process.kill();
            #[cfg(not(windows))]
            let stopped = process
                .kill_with(Signal::Term)
                .unwrap_or_else(|| process.kill());
            if !stopped {
                return Err(format!(
                    "Could not stop {}",
                    process.name().to_string_lossy()
                ));
            }
        }
    }
    for _ in 0..50 {
        if status()["running"] == false {
            return Ok(json!({"success": true}));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    return Err("ChatGPT desktop app did not exit; auth.json was not changed".into());
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
