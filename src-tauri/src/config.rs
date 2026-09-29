use crate::vault;
use chrono::Utc;
use serde_json::{json, Value};

fn config_path() -> Result<std::path::PathBuf, String> {
    Ok(vault::codex_dir()?.join("switcher_config.dat"))
}

pub fn load() -> Result<Value, String> {
    let path = config_path()?;
    let stored = match vault::read(&path)? {
        Some(value) => value,
        None => {
            let legacy = vault::codex_dir()?.join("switcher_config.json");
            vault::read(&legacy)?.unwrap_or_else(|| json!({}))
        }
    };
    let router_url = stored["routerUrl"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| std::env::var("ROUTER_URL").ok())
        .unwrap_or_default();
    let password = stored["password"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| std::env::var("ROUTER_PASSWORD").ok())
        .unwrap_or_default();
    let launch_desktop = stored["launchDesktopAfterSwitch"].as_bool().unwrap_or(true);
    Ok(json!({
        "routerUrl": router_url,
        "password": password,
        "launchDesktopAfterSwitch": launch_desktop,
        "hasPassword": !password.trim().is_empty()
    }))
}

pub fn save(router_url: String, password: String, launch_desktop: bool) -> Result<Value, String> {
    let url = router_url.trim().trim_end_matches('/');
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Router URL must begin with https:// or http://".into());
    }
    let value = json!({
        "routerUrl": url,
        "password": password,
        "launchDesktopAfterSwitch": launch_desktop,
        "updatedAt": Utc::now().to_rfc3339()
    });
    vault::write(&config_path()?, &value)?;
    Ok(value)
}
