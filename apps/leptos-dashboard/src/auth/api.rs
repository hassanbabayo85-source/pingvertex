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

pub async fn fetch_current_user_typed() -> Result<User, ApiError> {
    let parsed: CurrentSessionResponse = client::get_json("/auth/me").await?;
    Ok(parsed.user)
}

pub async fn fetch_current_user() -> Result<User, String> {
    fetch_current_user_typed().await.map_err(to_string_err)
}

// ---------------------------------------------------------------------
// Path 7 additions — email verification, password change, TOTP 2FA.
// Same conventions as the functions above: thin wrappers over
// `api::client`, errors flattened to `String` for simple form display,
// no JWT/session token ever touches this module directly (the browser
// only ever sees the HttpOnly `access_token` cookie set by the server).
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct EnableTotpResponse {
    otpauth_uri: String,
}

#[derive(Debug, Deserialize)]
struct ConfirmTotpSetupResponse {
    backup_codes: Vec<String>,
}

/// `POST /auth/verify-email` — public endpoint, proves ownership of the
/// emailed token rather than an active session.
pub async fn verify_email(token: &str) -> Result<(), String> {
    let body = json!({ "token": token });
    let _: Value = client::post_json("/auth/verify-email", &body)
        .await
        .map_err(to_string_err)?;
    Ok(())
}

/// `POST /auth/resend-verification` — the backend intentionally returns
/// the same 202 response whether or not the email exists/is already
/// verified (anti-enumeration), so the frontend should show a single
/// neutral "check your inbox" message regardless of the outcome here.
pub async fn resend_verification(email: &str) -> Result<(), String> {
    let body = json!({ "email": email });
    let _: Value = client::post_json("/auth/resend-verification", &body)
        .await
        .map_err(to_string_err)?;
    Ok(())
}

/// `POST /auth/password/change` — requires an authenticated session
/// (cookie) AND the caller's current password.
pub async fn change_password(current_password: &str, new_password: &str) -> Result<(), String> {
    let body = json!({
        "current_password": current_password,
        "new_password": new_password,
    });
    // The backend returns `204 No Content` on success. `client::post_json`
    // special-cases 204 by decoding an implied `null`, which deserializes
    // fine into `Value`, so we reuse it here rather than
    // `post_json_no_content` (which takes no request body — `/password/change`
    // needs one).
    let _: Value = client::post_json("/auth/password/change", &body)
        .await
        .map_err(to_string_err)?;
    Ok(())
}

/// `POST /auth/totp/enable` — starts 2FA enrollment, returns an
/// `otpauth://` URI to display for the user to add to their
/// authenticator app. Does NOT turn 2FA on yet.
pub async fn enable_totp() -> Result<String, String> {
    let parsed: EnableTotpResponse = client::post_json("/auth/totp/enable", &Value::Null)
        .await
        .map_err(to_string_err)?;
    Ok(parsed.otpauth_uri)
}

/// `POST /auth/totp/confirm` — verifies the first TOTP code and
/// permanently enables 2FA. Returns 10 backup codes shown exactly once;
/// the caller is responsible for making sure the user has a chance to
/// save them before navigating away.
pub async fn confirm_totp(totp_code: &str) -> Result<Vec<String>, String> {
    let body = json!({ "totp_code": totp_code });
    let parsed: ConfirmTotpSetupResponse = client::post_json("/auth/totp/confirm", &body)
        .await
        .map_err(to_string_err)?;
    Ok(parsed.backup_codes)
}

/// `POST /auth/totp/disable` — step-up re-authentication: requires the
/// account's CURRENT password in the body even though the caller already
/// has a valid session cookie, because disabling 2FA is treated as
/// sensitive as changing the password itself.
pub async fn disable_totp(password: &str) -> Result<(), String> {
    let body = json!({ "password": password });
    let _: Value = client::post_json("/auth/totp/disable", &body)
        .await
        .map_err(to_string_err)?;
    Ok(())
}
