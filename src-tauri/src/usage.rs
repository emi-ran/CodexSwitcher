use reqwest::header::{ACCEPT, AUTHORIZATION, ORIGIN, REFERER, USER_AGENT};
use serde_json::{json, Value};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

async fn request(
    client: &reqwest::Client,
    access_token: &str,
    account_id: Option<&str>,
) -> Result<reqwest::Response, String> {
    let mut call = client
        .get(USAGE_URL)
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .header(USER_AGENT, "CodexSwitcher/0.1")
        .header(ORIGIN, "https://chatgpt.com")
        .header(REFERER, "https://chatgpt.com")
        .header(ACCEPT, "application/json, text/plain, */*");
    if let Some(id) = account_id {
        call = call.header("chatgpt-account-id", id);
    }
    call.send()
        .await
        .map_err(|e| format!("Could not fetch usage: {e}"))
}

pub async fn fetch(client: &reqwest::Client, account: &mut Value) -> Result<Option<Value>, String> {
    let tokens = &account["tokens"];
    let Some(mut access_token) = tokens["access_token"].as_str().map(str::to_owned) else {
        return Ok(None);
    };
    let account_id = account["accountId"]
        .as_str()
        .or_else(|| tokens["account_id"].as_str())
        .map(str::to_owned);
    let mut response = request(client, &access_token, account_id.as_deref()).await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let Some(refresh_token) = account["tokens"]["refresh_token"].as_str() {
            let token_response = client
                .post(TOKEN_URL)
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("client_id", CLIENT_ID),
                    ("refresh_token", refresh_token),
                ])
                .send()
                .await
                .map_err(|e| format!("Could not refresh usage token: {e}"))?;
            if token_response.status().is_success() {
                let updated: Value = token_response
                    .json()
                    .await
                    .map_err(|_| "Invalid token refresh response")?;
                if let Some(token) = updated["access_token"].as_str() {
                    access_token = token.to_owned();
                    account["tokens"]["access_token"] = Value::String(access_token.clone());
                }
                for field in ["refresh_token", "id_token"] {
                    if let Some(token) = updated[field].as_str() {
                        account["tokens"][field] = Value::String(token.to_owned());
                    }
                }
                response = request(client, &access_token, account_id.as_deref()).await?;
            }
        }
    }
    if !response.status().is_success() {
        return Ok(Some(
            json!({"error": format!("Status {}", response.status().as_u16())}),
        ));
    }
    let data: Value = response
        .json()
        .await
        .map_err(|_| "Invalid usage response")?;
    apply_live_plan(account, &data);
    let rate = &data["rate_limit"];
    fn window(value: &Value, default_limit: u64) -> Value {
        if !value.is_object() {
            return Value::Null;
        }
        json!({
            "usedPercent": value["used_percent"].as_f64().unwrap_or(0.0).round(),
            "limitSeconds": value["limit_window_seconds"].as_u64().unwrap_or(default_limit),
            "resetAfterSeconds": value["reset_after_seconds"].as_u64().unwrap_or(0),
            "resetAt": value["reset_at"]
        })
    }
    Ok(Some(json!({
        "primary": window(&rate["primary_window"], 18000),
        "secondary": window(&rate["secondary_window"], 604800),
        "limitReached": rate["limit_reached"].as_bool().unwrap_or(false),
        "resetCredits": data["rate_limit_reset_credits"]["available_count"].as_u64().unwrap_or(0)
    })))
}

/// The plan in the stored id_token is frozen at login time, so the live usage
/// response is authoritative. A changed or free plan means the stored expiry
/// belongs to an old subscription and is cleared.
fn apply_live_plan(account: &mut Value, data: &Value) {
    let Some(plan) = data["plan_type"].as_str().filter(|value| !value.is_empty()) else {
        return;
    };
    let plan = plan.to_lowercase();
    let previous = account["plan"].as_str().map(str::to_owned);
    if previous.as_deref() != Some(plan.as_str()) || plan == "free" {
        account["expiresAt"] = Value::Null;
    }
    account["plan"] = Value::String(plan);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_free_plan_overrides_stored_plus_and_clears_expiry() {
        let mut account = json!({"plan": "plus", "expiresAt": "2026-12-01T00:00:00Z"});
        apply_live_plan(&mut account, &json!({"plan_type": "Free"}));
        assert_eq!(account["plan"], "free");
        assert_eq!(account["expiresAt"], Value::Null);
    }

    #[test]
    fn same_live_plan_keeps_expiry() {
        let mut account = json!({"plan": "plus", "expiresAt": "2026-12-01T00:00:00Z"});
        apply_live_plan(&mut account, &json!({"plan_type": "plus"}));
        assert_eq!(account["plan"], "plus");
        assert_eq!(account["expiresAt"], "2026-12-01T00:00:00Z");
    }

    #[test]
    fn missing_or_empty_live_plan_changes_nothing() {
        let original = json!({"plan": "plus", "expiresAt": "2026-12-01T00:00:00Z"});
        for data in [
            json!({}),
            json!({"plan_type": ""}),
            json!({"plan_type": null}),
        ] {
            let mut account = original.clone();
            apply_live_plan(&mut account, &data);
            assert_eq!(account, original);
        }
    }
}
