#![allow(dead_code)]
use gloo_net::http::Request;
use serde::Deserialize;
use serde_json::{json, Value};
use web_sys::RequestCredentials;

use crate::auth::context::User;

const API_BASE: &str = "/api/v1";

async fn post_json(path: &str, body: Value) -> Result<Value, String> {
    let url = format!("{}{}", API_BASE, path);
    let response = Request::post(&url)
        .header("Content-Type", "application/json")
        .credentials(RequestCredentials::Include)
        .body(serde_json::to_string(&body).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;

    if (200..300).contains(&status) {
        if text.is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_str(&text).map_err(|e| format!("Invalid JSON response: {e}"))
        }
    } else {
        Err(format!("Request failed with status {status}: {text}"))
    }
}

async fn get_json(path: &str) -> Result<Value, String> {
    let url = format!("{}{}", API_BASE, path);
    let response = Request::get(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;

    if (200..300).contains(&status) {
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON response: {e}"))
    } else {
        Err(format!("Request failed with status {status}: {text}"))
    }
}

#[derive(Debug, Clone)]
pub enum LoginResult {
    /// Login ya yi nasara; cookie an saita ta server. Kana buƙatar kira
    /// `fetch_current_user()` domin samo cikakken bayanin user.
    Success,
    TwoFactorRequired {
        pending_token: String,
    },
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    authenticated: bool,
    #[serde(default)]
    two_factor_required: bool,
    #[serde(default)]
    pending_token: Option<String>,
}

pub async fn login(email: &str, password: &str) -> Result<LoginResult, String> {
    let body = json!({ "email": email, "password": password });
    let value = post_json("/auth/login", body).await?;
    let parsed: LoginResponse =
        serde_json::from_value(value).map_err(|e| format!("Invalid login response: {e}"))?;

    if parsed.two_factor_required {
        let token = parsed
            .pending_token
            .ok_or_else(|| "Missing pending_token in response".to_string())?;
        Ok(LoginResult::TwoFactorRequired {
            pending_token: token,
        })
    } else if parsed.authenticated {
        Ok(LoginResult::Success)
    } else {
        Err("Login failed".to_string())
    }
}

pub async fn register(username: &str, email: &str, password: &str) -> Result<(), String> {
    let body = json!({
        "username": username,
        "email": email,
        "password": password,
    });
    post_json("/auth/register", body).await?;
    Ok(())
}

pub async fn logout() -> Result<(), String> {
    post_json("/auth/logout", json!({})).await?;
    Ok(())
}

pub async fn forgot_password(email: &str) -> Result<(), String> {
    let body = json!({ "email": email });
    post_json("/auth/forgot-password", body).await?;
    Ok(())
}

pub async fn reset_password(token: &str, new_password: &str) -> Result<(), String> {
    let body = json!({
        "token": token,
        "new_password": new_password,
    });
    post_json("/auth/reset-password", body).await?;
    Ok(())
}

pub async fn complete_2fa(pending_token: &str, code: &str) -> Result<(), String> {
    let body = json!({
        "pending_token": pending_token,
        "code": code,
    });
    post_json("/auth/totp/verify", body).await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct CurrentSessionResponse {
    #[allow(dead_code)]
    authenticated: bool,
    user: User,
}

pub async fn fetch_current_user() -> Result<User, String> {
    let value = get_json("/auth/me").await?;
    let parsed: CurrentSessionResponse =
        serde_json::from_value(value).map_err(|e| format!("Invalid session data: {e}"))?;
    Ok(parsed.user)
}
