use crate::auth;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::header::{ACCEPT, COOKIE, ORIGIN, REFERER, SET_COOKIE, USER_AGENT};
use serde_json::{json, Value};

const AGENT: &str = "CodexSwitcher/0.1";

pub fn clean_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_owned()
}

pub async fn fetch_database(
    client: &reqwest::Client,
    url: &str,
    password: &str,
) -> Result<Value, String> {
    let base = clean_url(url);
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        return Err("9Router URL is invalid".into());
    }
    if password.is_empty() {
        return Err("9Router password is required".into());
    }
    let login = client
        .post(format!("{base}/api/auth/login"))
        .header(USER_AGENT, AGENT)
        .header(ORIGIN, &base)
        .header(REFERER, format!("{base}/login"))
        .json(&json!({"password": password, "rememberMe": false}))
        .send()
        .await
        .map_err(|e| format!("Could not connect to 9Router: {e}"))?;
    if !login.status().is_success() {
        return Err(format!("9Router login failed (HTTP {})", login.status()));
    }
    let cookies: Vec<String> = login
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .filter_map(|header| header.split(';').next())
        .map(str::to_owned)
        .collect();
    let login_data: Value = login
        .json()
        .await
        .map_err(|_| "Invalid 9Router login response")?;
    if let Some(error) = login_data["error"].as_str() {
        return Err(format!("9Router login failed: {error}"));
    }
    let response = client
        .get(format!("{base}/api/settings/database"))
        .header(USER_AGENT, AGENT)
        .header(ACCEPT, "*/*")
        .header(ORIGIN, &base)
        .header(REFERER, format!("{base}/dashboard/profile"))
        .header(COOKIE, cookies.join("; "))
        .header("x-9r-password", password)
        .send()
        .await
        .map_err(|e| format!("Could not fetch 9Router database: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "9Router database request failed (HTTP {})",
            response.status()
        ));
    }
    let database: Value = response
        .json()
        .await
        .map_err(|_| "Invalid 9Router database response")?;
    if let Some(error) = database["error"].as_str() {
        return Err(format!("9Router database error: {error}"));
    }
    Ok(database)
}

fn claim(token: &str) -> Option<Value> {
    let data = URL_SAFE_NO_PAD.decode(token.split('.').nth(1)?).ok()?;
    serde_json::from_slice(&data).ok()
}

pub fn extract_accounts(database: &Value) -> Vec<Value> {
    let mut accounts = Vec::new();
    let Some(connections) = database["providerConnections"].as_array() else {
        return accounts;
    };
    for connection in connections {
        let is_codex = connection["provider"]
            .as_str()
            .is_some_and(|value| value.eq_ignore_ascii_case("codex"))
            || (connection["authType"] == "oauth"
                && connection["providerSpecificData"]["chatgptAccountId"].is_string());
        if !is_codex {
            continue;
        }
        let psd = &connection["providerSpecificData"];
        let tokens = &connection["tokens"];
        let id_token = first_str(&[
            &connection["idToken"],
            &connection["id_token"],
            &tokens["id_token"],
        ])
        .unwrap_or_default();
        let access_token = first_str(&[
            &connection["accessToken"],
            &connection["access_token"],
            &tokens["access_token"],
        ])
        .unwrap_or_default();
        let refresh_token = first_str(&[
            &connection["refreshToken"],
            &connection["refresh_token"],
            &tokens["refresh_token"],
        ])
        .unwrap_or_default();
        let jwt = claim(&id_token);
        let jwt_auth = jwt
            .as_ref()
            .map(|value| &value["https://api.openai.com/auth"]);
        let account_id = jwt_auth
            .and_then(|value| value["chatgpt_account_id"].as_str())
            .map(str::to_owned)
            .or_else(|| {
                first_str(&[
                    &psd["chatgptAccountId"],
                    &connection["accountId"],
                    &connection["account_id"],
                    &tokens["account_id"],
                ])
            });
        let email = jwt
            .as_ref()
            .and_then(|value| value["email"].as_str())
            .map(str::to_owned)
            .or_else(|| first_str(&[&connection["email"], &connection["name"]]))
            .unwrap_or_else(|| "No email associated".into());
        let plan = jwt_auth
            .and_then(|value| value["chatgpt_plan_type"].as_str())
            .map(str::to_owned)
            .or_else(|| first_str(&[&psd["chatgptPlanType"], &connection["plan"]]))
            .unwrap_or_else(|| "plus".into());
        let id = connection["id"].as_str().unwrap_or_default().to_owned();
        let name = first_str(&[&connection["name"]]).unwrap_or_else(|| email.clone());
        accounts.push(json!({
            "id": id, "name": name, "email": email, "plan": plan.to_lowercase(),
            "accountId": account_id, "authMode": "chatgpt",
            "expiresAt": jwt_auth.and_then(|value| value["chatgpt_subscription_active_until"].as_str())
                .or_else(|| psd["chatgptSubscriptionActiveUntil"].as_str()),
            "tokens": {"id_token": id_token, "access_token": access_token, "refresh_token": refresh_token, "account_id": account_id},
            "apiKey": Value::Null, "isActive": false, "providerName": "9Router Codex"
        }));
    }
    accounts
}

fn first_str(values: &[&Value]) -> Option<String> {
    values.iter().find_map(|value| {
        value
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

pub fn mark_active(accounts: &mut [Value], active: Option<&Value>) {
    for account in accounts {
        let matched = active.is_some_and(|current| {
            let id_match = current["accountId"]
                .as_str()
                .is_some_and(|id| account["accountId"] == id);
            let email_match = current["email"].as_str().is_some_and(|email| {
                account["email"]
                    .as_str()
                    .is_some_and(|value| value.eq_ignore_ascii_case(email))
            });
            let token_match = current["tokens"]["refresh_token"]
                .as_str()
                .is_some_and(|token| {
                    !token.is_empty() && account["tokens"]["refresh_token"] == token
                });
            id_match || email_match || token_match
        });
        account["isActive"] = Value::Bool(matched);
    }
}

pub fn public_accounts(accounts: &[Value]) -> Vec<Value> {
    accounts
        .iter()
        .map(|account| {
            let mut sanitized = account.clone();
            if let Some(object) = sanitized.as_object_mut() {
                object.remove("tokens");
                object.remove("apiKey");
            }
            sanitized
        })
        .collect()
}

pub fn active_public(active: Option<&Value>) -> Option<Value> {
    active.map(auth::public_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_accounts_are_filtered_and_redacted_for_frontend() {
        let database = json!({"providerConnections": [
            {"id": "codex-1", "provider": "codex", "email": "person@example.test", "accessToken": "dummy-access", "refreshToken": "dummy-refresh"},
            {"id": "other-1", "provider": "gemini", "accessToken": "other-token"}
        ]});
        let accounts = extract_accounts(&database);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0]["tokens"]["access_token"], "dummy-access");
        let public = public_accounts(&accounts);
        assert_eq!(public[0]["email"], "person@example.test");
        assert!(public[0].get("tokens").is_none());
        assert!(public[0].get("apiKey").is_none());
    }
}
