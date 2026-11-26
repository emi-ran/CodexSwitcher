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
            .unwrap_or_else(|| "unknown".into());
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

const PLACEHOLDER_EMAIL: &str = "No email associated";

fn non_empty(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.is_empty())
}

fn real_email(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|email| !email.is_empty() && !email.eq_ignore_ascii_case(PLACEHOLDER_EMAIL))
        .map(str::to_ascii_lowercase)
}

/// Returns the match tier (1 = best) of `account` against the active auth, or None.
///   1: non-empty refresh token is equal.
///   2: accountId equal AND email equal (case-insensitive).
///   3: partial match without contradiction: accountId equal where one side has
///      no email, or email equal where one side has no accountId.
fn match_tier(current: &Value, account: &Value) -> Option<u8> {
    let current_token = non_empty(&current["tokens"]["refresh_token"]);
    if current_token.is_some() && current_token == non_empty(&account["tokens"]["refresh_token"]) {
        return Some(1);
    }
    let current_id = non_empty(&current["accountId"]);
    let account_id = non_empty(&account["accountId"]);
    let current_email = real_email(&current["email"]);
    let account_email = real_email(&account["email"]);
    let id_equal = current_id.is_some() && current_id == account_id;
    let email_equal = current_email.is_some() && current_email == account_email;
    if id_equal && email_equal {
        return Some(2);
    }
    if id_equal && (current_email.is_none() || account_email.is_none()) {
        return Some(3);
    }
    if email_equal && (current_id.is_none() || account_id.is_none()) {
        return Some(3);
    }
    None
}

/// Marks only the accounts in the best (lowest) tier that has any match.
pub fn mark_active(accounts: &mut [Value], active: Option<&Value>) {
    let tiers: Vec<Option<u8>> = accounts
        .iter()
        .map(|account| active.and_then(|current| match_tier(current, account)))
        .collect();
    let best = tiers.iter().flatten().min().copied();
    for (account, tier) in accounts.iter_mut().zip(tiers) {
        account["isActive"] = Value::Bool(best.is_some() && tier == best);
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

    fn account(account_id: Value, email: Value, refresh_token: &str) -> Value {
        json!({
            "accountId": account_id,
            "email": email,
            "tokens": {"refresh_token": refresh_token},
            "isActive": false
        })
    }

    fn active_ids(accounts: &[Value]) -> Vec<bool> {
        accounts
            .iter()
            .map(|account| account["isActive"] == true)
            .collect()
    }

    #[test]
    fn shared_account_id_with_different_emails_marks_only_same_email() {
        let current = account(json!("acc-team"), json!("a@example.test"), "");
        let mut accounts = vec![
            account(json!("acc-team"), json!("a@example.test"), "rt-a"),
            account(json!("acc-team"), json!("b@example.test"), "rt-b"),
        ];
        mark_active(&mut accounts, Some(&current));
        assert_eq!(active_ids(&accounts), vec![true, false]);
    }

    #[test]
    fn same_email_with_different_account_ids_marks_only_same_account_id() {
        let current = account(json!("acc-1"), json!("same@example.test"), "");
        let mut accounts = vec![
            account(json!("acc-2"), json!("same@example.test"), "rt-2"),
            account(json!("acc-1"), json!("SAME@example.test"), "rt-1"),
        ];
        mark_active(&mut accounts, Some(&current));
        assert_eq!(active_ids(&accounts), vec![false, true]);
    }

    #[test]
    fn token_match_wins_over_account_id_and_email_match() {
        let current = account(json!("acc-1"), json!("a@example.test"), "rt-current");
        let mut accounts = vec![
            account(json!("acc-1"), json!("a@example.test"), "rt-other"),
            account(json!("acc-9"), json!("b@example.test"), "rt-current"),
        ];
        mark_active(&mut accounts, Some(&current));
        assert_eq!(active_ids(&accounts), vec![false, true]);
    }

    #[test]
    fn partial_match_is_used_when_no_stronger_match_exists() {
        // Current auth has an accountId but no email; the placeholder email on the
        // account side is treated as missing, so the partial tier applies.
        let current = account(json!("acc-1"), Value::Null, "");
        let mut accounts = vec![
            account(json!("acc-1"), json!("No email associated"), "rt-1"),
            account(json!("acc-2"), json!("other@example.test"), "rt-2"),
        ];
        mark_active(&mut accounts, Some(&current));
        assert_eq!(active_ids(&accounts), vec![true, false]);

        // Current auth has an email but no accountId; the account matches on email.
        let current = account(Value::Null, json!("a@example.test"), "");
        let mut accounts = vec![
            account(json!("acc-1"), json!("a@example.test"), "rt-1"),
            account(json!("acc-2"), json!("b@example.test"), "rt-2"),
        ];
        mark_active(&mut accounts, Some(&current));
        assert_eq!(active_ids(&accounts), vec![true, false]);
    }

    #[test]
    fn no_active_account_clears_all_flags() {
        let mut accounts = vec![
            account(json!("acc-1"), json!("a@example.test"), "rt-1"),
            account(json!("acc-2"), json!("b@example.test"), "rt-2"),
        ];
        accounts[0]["isActive"] = Value::Bool(true);
        mark_active(&mut accounts, None);
        assert_eq!(active_ids(&accounts), vec![false, false]);

        let current = account(json!("acc-3"), json!("c@example.test"), "rt-3");
        mark_active(&mut accounts, Some(&current));
        assert_eq!(active_ids(&accounts), vec![false, false]);
    }
}
