#![allow(dead_code)]
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::client::{self, ApiError};
use crate::auth::context::User;

fn to_string_err(e: ApiError) -> String {
    e.to_string()
}

#[derive(Debug, Clone)]
pub enum LoginResult {
    /// Login succeeded; cookie set by server. Call `fetch_current_user()`
    /// to obtain the full user profile.
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

#[derive(Debug, Deserialize)]
struct CurrentSessionResponse {
    #[allow(dead_code)]
    authenticated: bool,
    user: User,
}

pub async fn login(email: &str, password: &str) -> Result<LoginResult, String> {
    let body = json!({ "email": email, "password": password });
    let parsed: LoginResponse = client::post_json("/auth/login", &body)
        .await
        .map_err(to_string_err)?;

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
    let _: Value = client::post_json("/auth/register", &body)
        .await
        .map_err(to_string_err)?;
    Ok(())
}

pub async fn logout() -> Result<(), String> {
    client::post_json_no_content("/auth/logout")
        .await
        .map_err(to_string_err)
}

pub async fn forgot_password(email: &str) -> Result<(), String> {
    let body = json!({ "email": email });
    let _: Value = client::post_json("/auth/forgot-password", &body)
        .await
        .map_err(to_string_err)?;
    Ok(())
}

pub async fn reset_password(token: &str, new_password: &str) -> Result<(), String> {
    let body = json!({
        "token": token,
        "new_password": new_password,
    });
    let _: Value = client::post_json("/auth/reset-password", &body)
        .await
        .map_err(to_string_err)?;
    Ok(())
}

pub async fn complete_2fa(pending_token: &str, code: &str) -> Result<(), String> {
    let body = json!({
        "pending_token": pending_token,
        "code": code,
    });
    let _: Value = client::post_json("/auth/totp/verify", &body)
        .await
        .map_err(to_string_err)?;
    Ok(())
}

pub async fn fetch_current_user() -> Result<User, String> {
    let parsed: CurrentSessionResponse =
        client::get_json("/auth/me").await.map_err(to_string_err)?;
    Ok(parsed.user)
}
