mod auth;
mod config;
mod desktop;
mod router;
mod usage;
mod vault;

use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;

struct AppState {
    accounts: Mutex<Vec<Value>>,
    client: reqwest::Client,
}

fn saved_accounts() -> Result<Vec<Value>, String> {
    Ok(
        vault::read(&vault::codex_dir()?.join("switcher_accounts.dat"))?
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default(),
    )
}

fn persist_accounts(accounts: &[Value]) -> Result<(), String> {
    vault::write(
        &vault::codex_dir()?.join("switcher_accounts.dat"),
        &json!(accounts),
    )
}

fn public_active(active: Option<&Value>, accounts: &[Value]) -> Option<Value> {
    router::active_public(active).map(|mut info| {
        if let Some(account) = accounts.iter().find(|account| account["isActive"] == true) {
            info["usage"] = account["usage"].clone();
        }
        info
    })
}

#[tauri::command]
fn get_status(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let settings = config::load()?;
    let active = auth::current()?;
    let mut accounts = state.accounts.lock().map_err(|e| e.to_string())?.clone();
    router::mark_active(&mut accounts, active.as_ref());
    let active_public = public_active(active.as_ref(), &accounts);
    let public_accounts = router::public_accounts(&accounts);
    Ok(json!({
        "success": true,
        "routerConfig": {"url": settings["routerUrl"], "hasPassword": settings["hasPassword"], "launchDesktopAfterSwitch": settings["launchDesktopAfterSwitch"]},
        "activeAccount": active_public,
        "codexStatus": desktop::status(),
        "cachedAccountsCount": public_accounts.len(),
        "accounts": public_accounts
    }))
}

#[tauri::command]
async fn sync_accounts(
    state: tauri::State<'_, AppState>,
    password: Option<String>,
) -> Result<Value, String> {
    let settings = config::load()?;
    let url = settings["routerUrl"]
        .as_str()
        .ok_or("Router URL is unavailable")?;
    if url.is_empty() {
        return Ok(
            json!({"success": false, "requiresConfiguration": true, "error": "9Router URL is required"}),
        );
    }
    let password = password
        .or_else(|| settings["password"].as_str().map(str::to_owned))
        .unwrap_or_default();
    if password.is_empty() {
        return Ok(
            json!({"success": false, "requiresPassword": true, "error": "9Router password is required"}),
        );
    }
    let database = router::fetch_database(&state.client, url, &password).await?;
    let mut accounts = router::extract_accounts(&database);
    let active = auth::current()?;
    router::mark_active(&mut accounts, active.as_ref());
    for account in &mut accounts {
        match usage::fetch(&state.client, account).await {
            Ok(Some(data)) => account["usage"] = data,
            Ok(None) => {}
            Err(error) => account["usage"] = json!({"error": error}),
        }
    }
    persist_accounts(&accounts)?;
    *state.accounts.lock().map_err(|e| e.to_string())? = accounts.clone();
    Ok(json!({
        "success": true,
        "accounts": router::public_accounts(&accounts),
        "activeAccount": public_active(active.as_ref(), &accounts),
        "routerUrl": router::clean_url(url)
    }))
}

#[tauri::command]
fn switch_account(state: tauri::State<'_, AppState>, account_id: String) -> Result<Value, String> {
    let mut accounts = state.accounts.lock().map_err(|e| e.to_string())?;
    let account = accounts
        .iter()
        .find(|account| account["id"] == account_id)
        .cloned()
        .ok_or("Selected account was not found in the encrypted cache")?;
    if !account["tokens"]["access_token"]
        .as_str()
        .is_some_and(|token| !token.is_empty())
    {
        return Err("Selected account has no usable OAuth credentials".into());
    }
    let was_running = desktop::status()["running"].as_bool().unwrap_or(false);
    let mut steps = Vec::new();
    if was_running {
        desktop::stop()?;
        steps.push("ChatGPT desktop app closed");
    }
    let backup = auth::write_account(&account)?;
    steps.push("auth.json backed up and updated");
    let settings = config::load()?;
    let mut launched = false;
    if settings["launchDesktopAfterSwitch"] == true {
        match desktop::start() {
            Ok(_) => {
                launched = true;
                steps.push("ChatGPT desktop app launch requested");
            }
            Err(_) => steps.push("ChatGPT desktop app could not be opened automatically"),
        }
    }
    let active = auth::current()?;
    router::mark_active(&mut accounts, active.as_ref());
    let cache_warning = persist_accounts(&accounts).err();
    Ok(json!({
        "success": true,
        "switchResult": {"success": true, "wasRunning": was_running, "steps": steps, "backupPath": backup, "launched": launched, "cacheWarning": cache_warning},
        "activeAccount": public_active(active.as_ref(), &accounts),
        "accounts": router::public_accounts(&accounts)
    }))
}

#[tauri::command]
fn stop_desktop() -> Result<Value, String> {
    desktop::stop()
}

#[tauri::command]
fn start_desktop() -> Result<Value, String> {
    desktop::start()
}

#[tauri::command]
fn get_settings() -> Result<Value, String> {
    let loaded = config::load()?;
    Ok(json!({
        "routerConfig": {
            "url": loaded["routerUrl"],
            "hasPassword": loaded["hasPassword"],
            "launchDesktopAfterSwitch": loaded["launchDesktopAfterSwitch"]
        }
    }))
}

#[tauri::command]
fn save_settings(
    router_url: String,
    password: String,
    launch_desktop_after_switch: bool,
) -> Result<Value, String> {
    let saved = config::save(router_url, password, launch_desktop_after_switch)?;
    Ok(json!({
        "routerConfig": {
            "url": saved["routerUrl"],
            "hasPassword": saved["password"].as_str().is_some_and(|value| !value.trim().is_empty()),
            "launchDesktopAfterSwitch": saved["launchDesktopAfterSwitch"]
        }
    }))
}

#[tauri::command]
fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())?;
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())?;
    }
    get_autostart(app)
}

pub fn run() {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("HTTP client could not be created");
    let accounts = saved_accounts().unwrap_or_default();
    tauri::Builder::default()
        .manage(AppState {
            accounts: Mutex::new(accounts),
            client,
        })
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden".into()]),
        ))
        .setup(|app| {
            let open = MenuItem::with_id(app, "open", "Open Codex Switcher", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .expect("App icon is missing")
                        .clone(),
                )
                .tooltip("Codex Switcher")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;
            if std::env::args().any(|argument| argument == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    window.hide()?;
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_autostart,
            set_autostart,
            get_status,
            sync_accounts,
            switch_account,
            stop_desktop,
            start_desktop
        ])
        .run(tauri::generate_context!())
        .expect("Could not start CodexSwitcher");
}
