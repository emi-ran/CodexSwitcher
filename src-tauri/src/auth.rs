use crate::vault;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

pub fn path() -> Result<PathBuf, String> {
    Ok(vault::codex_dir()?.join("auth.json"))
}

fn claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn current() -> Result<Option<Value>, String> {
    let raw = match fs::read(path()?) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let data: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let tokens = &data["tokens"];
    let token_claims = tokens["id_token"].as_str().and_then(claims);
    let auth_claims = token_claims
        .as_ref()
        .map(|value| &value["https://api.openai.com/auth"]);
    Ok(Some(json!({
        "authMode": data["auth_mode"].as_str().unwrap_or(if data["OPENAI_API_KEY"].is_string() { "api_key" } else { "chatgpt" }),
        "email": token_claims.as_ref().and_then(|value| value["email"].as_str()),
        "plan": auth_claims.and_then(|value| value["chatgpt_plan_type"].as_str()),
        "accountId": auth_claims.and_then(|value| value["chatgpt_account_id"].as_str()).or_else(|| tokens["account_id"].as_str()),
        "expiresAt": auth_claims.and_then(|value| value["chatgpt_subscription_active_until"].as_str()),
        "hasTokens": tokens.is_object(),
        "hasApiKey": data["OPENAI_API_KEY"].as_str().is_some_and(|value| !value.is_empty()),
        "lastRefresh": data["last_refresh"],
        "tokens": tokens,
        "raw": data
    })))
}

pub fn public_info(info: &Value) -> Value {
    json!({
        "authMode": info["authMode"],
        "email": info["email"],
        "plan": info["plan"],
        "accountId": info["accountId"],
        "expiresAt": info["expiresAt"],
        "hasTokens": info["hasTokens"],
        "hasApiKey": info["hasApiKey"],
        "lastRefresh": info["lastRefresh"],
        "usage": info["usage"]
    })
}

pub fn write_account(account: &Value) -> Result<Option<PathBuf>, String> {
    let tokens = &account["tokens"];
    if !tokens.is_object() || !tokens["access_token"].is_string() {
        return Err("Selected account has no usable OAuth credentials".into());
    }
    let destination = path()?;
    let backup = if destination.exists() {
        let backup_dir = vault::codex_dir()?.join("backups");
        fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
        let backup_file = backup_dir.join(format!(
            "auth.json.bak.{}",
            Utc::now().format("%Y-%m-%dT%H-%M-%S-%fZ")
        ));
        fs::copy(&destination, &backup_file)
            .map_err(|e| format!("Could not back up current auth.json: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&backup_file, fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("Could not protect auth.json backup: {e}"))?;
        }
        Some(backup_file)
    } else {
        None
    };
    let target = json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": Value::Null,
        "tokens": tokens,
        "last_refresh": Utc::now().to_rfc3339()
    });
    let bytes = serde_json::to_vec_pretty(&target).map_err(|e| e.to_string())?;
    vault::atomic_write_bytes(&destination, &bytes)?;
    Ok(backup)
}
