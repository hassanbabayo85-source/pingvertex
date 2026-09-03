#!/usr/bin/env bash
set -euo pipefail

# PingVertex — Phase 1 Backend ↔ Frontend Integration Hardening
# Generated from the supplied source snapshot after source-code inspection.
# Run from the PingVertex project root. Existing target files are backed up.

ROOT="${1:-$PWD}"
cd "$ROOT"

if [[ ! -f Cargo.toml || ! -d crates || ! -d apps/leptos-dashboard ]]; then
  echo "ERROR: run this script from the PingVertex project root."
  exit 1
fi

STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP_DIR=".phase1-backup-${STAMP}"
mkdir -p "$BACKUP_DIR"

backup() {
  local path="$1"
  if [[ -f "$path" ]]; then
    mkdir -p "$BACKUP_DIR/$(dirname "$path")"
    cp -a "$path" "$BACKUP_DIR/$path"
  fi
}


backup "apps/leptos-dashboard/src/api/monitoring.rs"

backup "apps/leptos-dashboard/src/api/client.rs"

backup "apps/leptos-dashboard/src/auth/api.rs"

backup "apps/leptos-dashboard/src/components/require_auth.rs"

backup "apps/leptos-dashboard/src/pages/dashboard_overview.rs"

backup "crates/infra/src/repositories/postgres_dashboard_repository.rs"

backup "crates/infra/tests/postgres_dashboard_repository.rs"

backup "docs/FRONTEND_BACKEND_CONTRACT_AUDIT_2026-09-03.md"

backup "scripts/audit_frontend_backend_contracts.sh"

backup "HANDOVER.md"

backup "apps/leptos-dashboard/HANDOVER.md"

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/monitoring.rs")"
cat > "apps/leptos-dashboard/src/api/monitoring.rs" <<'PINGVERTEX_PHASE1_EOF'
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

use crate::api::client::{self, ApiError};

fn to_string_err(e: ApiError) -> String {
    e.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Monitor {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub url: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub enabled: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub status: Option<String>,
    pub uptime_percentage: Option<f64>,
    pub avg_response_time_ms: Option<f64>,
    pub expected_status: u16,
    pub is_public: bool,
    pub ssl_check_enabled: bool,
    pub ssl_expiry_warning_days: u32,
    pub tcp_port: Option<u16>,
    pub dns_record_type: Option<String>,
    pub dns_expected_value: Option<String>,
    pub heartbeat_interval_seconds: Option<u32>,
    pub heartbeat_grace_period_seconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct MonitorSummary {
    pub total: u32,
    pub up: u32,
    pub down: u32,
    pub disabled: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IncidentSummary {
    pub id: String,
    pub monitor_id: String,
    pub started_at: String,
    pub acknowledged_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IncidentSummaryData {
    pub open_count: u32,
    pub recent: Vec<IncidentSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MonitorOverviewItem {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub status: Option<String>,
    pub latency_ms: Option<u64>,
    pub checked_at: Option<String>,
    pub open_incident: Option<IncidentSummary>,
}

/// Exact wire contract of GET /api/v1/dashboard/overview.
/// Keep this aligned with application::dto::DashboardOverview.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DashboardOverview {
    pub monitor_summary: MonitorSummary,
    pub monitors: Vec<MonitorOverviewItem>,
    pub incident_summary: IncidentSummaryData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Check {
    pub id: String,
    pub monitor_id: String,
    pub status: String,
    pub response_time_ms: Option<u64>,
    pub checked_at: String,
    pub region: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Incident {
    pub id: String,
    pub monitor_id: String,
    pub status: String,
    pub title: String,
    pub description: Option<String>,
    pub started_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct NotificationChannel {
    pub id: String,
    pub monitor_id: String,
    pub kind: String,
    pub target: String,
    pub enabled: bool,
    #[serde(default)]
    pub name: String,
    pub has_signing_secret: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct NotificationChannelCreateResponse {
    #[serde(flatten)]
    pub channel: NotificationChannel,
    pub signing_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonitorPayload {
    pub name: String,
    pub kind: String,
    pub url: String,
    pub method: String,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    #[serde(default)]
    pub assertions: Vec<serde_json::Value>,
    pub is_public: bool,
    pub ssl_check_enabled: bool,
    pub ssl_expiry_warning_days: u32,
    pub tcp_port: Option<u16>,
    pub dns_record_type: Option<String>,
    pub dns_expected_value: Option<String>,
    pub heartbeat_interval_seconds: Option<u32>,
    pub heartbeat_grace_period_seconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelPayload {
    #[serde(default)]
    pub name: String,
    pub kind: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MaintenanceWindowPayload {
    pub starts_at: String,
    pub ends_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EscalationStepPayload {
    pub delay_minutes: u32,
    pub channel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EscalationPolicyPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_id: Option<String>,
    pub steps: Vec<EscalationStepPayload>,
}

pub async fn fetch_monitors() -> Result<Vec<Monitor>, String> {
    client::get_json::<Vec<Monitor>>("/monitors")
        .await
        .map_err(to_string_err)
}

pub async fn get_monitor(id: &str) -> Result<Monitor, String> {
    client::get_json::<Monitor>(&format!("/monitors/{id}"))
        .await
        .map_err(to_string_err)
}

pub async fn create_monitor(payload: &MonitorPayload) -> Result<Monitor, String> {
    client::post_json::<Monitor, _>("/monitors", payload)
        .await
        .map_err(to_string_err)
}

pub async fn update_monitor(id: &str, payload: &MonitorPayload) -> Result<Monitor, String> {
    client::put_json::<Monitor, _>(&format!("/monitors/{id}"), payload)
        .await
        .map_err(to_string_err)
}

pub async fn delete_monitor(id: &str) -> Result<(), String> {
    client::delete_json_no_content(&format!("/monitors/{id}"))
        .await
        .map_err(to_string_err)
}

pub async fn set_monitor_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let path = if enabled {
        format!("/monitors/{id}/enable")
    } else {
        format!("/monitors/{id}/disable")
    };
    client::post_json_no_content(&path)
        .await
        .map_err(to_string_err)
}

pub async fn fetch_overview() -> Result<DashboardOverview, String> {
    client::get_json::<DashboardOverview>("/dashboard/overview")
        .await
        .map_err(to_string_err)
}

pub async fn fetch_incidents(monitor_id: &str) -> Result<Vec<Incident>, String> {
    client::get_json::<Vec<Incident>>(&format!("/monitors/{monitor_id}/incidents"))
        .await
        .map_err(to_string_err)
}

pub async fn fetch_checks(monitor_id: &str) -> Result<Vec<Check>, String> {
    client::get_json::<Vec<Check>>(&format!("/monitors/{monitor_id}/checks"))
        .await
        .map_err(to_string_err)
}

pub async fn list_notification_channels(
    monitor_id: &str,
) -> Result<Vec<NotificationChannel>, String> {
    client::get_json::<Vec<NotificationChannel>>(&format!("/monitors/{monitor_id}/notifications"))
        .await
        .map_err(to_string_err)
}

pub async fn create_notification_channel(
    monitor_id: &str,
    payload: &ChannelPayload,
) -> Result<NotificationChannelCreateResponse, String> {
    client::post_json::<NotificationChannelCreateResponse, _>(
        &format!("/monitors/{monitor_id}/notifications"),
        payload,
    )
    .await
    .map_err(to_string_err)
}

pub async fn delete_notification_channel(channel_id: &str) -> Result<(), String> {
    client::delete_json_no_content(&format!("/notifications/{channel_id}"))
        .await
        .map_err(to_string_err)
}

pub async fn test_notification_channel(channel_id: &str) -> Result<(), String> {
    client::post_json_no_content(&format!("/notifications/{channel_id}/test"))
        .await
        .map_err(to_string_err)
}

pub async fn create_maintenance_window(
    monitor_id: &str,
    payload: &MaintenanceWindowPayload,
) -> Result<(), String> {
    let _: serde_json::Value = client::post_json(
        &format!("/monitors/{monitor_id}/maintenance-windows"),
        payload,
    )
    .await
    .map_err(to_string_err)?;
    Ok(())
}

pub async fn create_escalation_policy(payload: &EscalationPolicyPayload) -> Result<(), String> {
    let _: serde_json::Value = client::post_json("/escalation-policies", payload)
        .await
        .map_err(to_string_err)?;
    Ok(())
}

pub async fn acknowledge_incident(incident_id: &str) -> Result<(), String> {
    client::post_json_no_content(&format!("/incidents/{incident_id}/acknowledge"))
        .await
        .map_err(to_string_err)
}

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/client.rs")"
cat > "apps/leptos-dashboard/src/api/client.rs" <<'PINGVERTEX_PHASE1_EOF'
use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use web_sys::{FormData, RequestCredentials};

const API_BASE: &str = "/api/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    Validation(String),
    NotFound,
    Conflict(String),
    RateLimited { message: String, retry_after: Option<u64> },
    Network(String),
    Server(u16, String),
    Unknown(u16, String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "Unauthorized (401)"),
            Self::Forbidden => write!(f, "Forbidden (403)"),
            Self::Validation(msg) => write!(f, "Validation error: {msg}"),
            Self::NotFound => write!(f, "Not found (404)"),
            Self::Conflict(msg) => write!(f, "Conflict (409): {msg}"),
            Self::RateLimited { message, retry_after } => match retry_after {
                Some(seconds) => write!(f, "Rate limited (429): {message} (retry after {seconds}s)"),
                None => write!(f, "Rate limited (429): {message}"),
            },
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::Server(status, msg) => write!(f, "Server error ({status}): {msg}"),
            Self::Unknown(status, msg) => write!(f, "Unexpected HTTP error ({status}): {msg}"),
        }
    }
}

impl From<gloo_net::Error> for ApiError {
    fn from(err: gloo_net::Error) -> Self { Self::Network(err.to_string()) }
}

#[derive(Debug, Deserialize)]
struct ErrorBody { message: Option<String> }

fn safe_message(status: u16, body: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<ErrorBody>(body) {
        if let Some(message) = parsed.message.filter(|m| !m.trim().is_empty()) {
            return if (500..=599).contains(&status) {
                "The server could not complete the request.".to_string()
            } else { message };
        }
    }
    if (500..=599).contains(&status) {
        "The server could not complete the request.".to_string()
    } else if body.trim().is_empty() {
        "The request could not be completed.".to_string()
    } else {
        body.trim().chars().take(500).collect()
    }
}

fn parse_retry_after(value: Option<&str>) -> Option<u64> {
    value.and_then(|v| v.trim().parse::<u64>().ok())
}

fn map_status(status: u16, body: String, retry_after: Option<u64>) -> ApiError {
    let message = safe_message(status, &body);
    match status {
        401 => ApiError::Unauthorized,
        403 => ApiError::Forbidden,
        400 | 422 => ApiError::Validation(message),
        404 => ApiError::NotFound,
        409 => ApiError::Conflict(message),
        429 => ApiError::RateLimited { message, retry_after },
        500..=599 => ApiError::Server(status, message),
        _ => ApiError::Unknown(status, message),
    }
}

async fn handle_response<T: DeserializeOwned>(resp: gloo_net::http::Response) -> Result<T, ApiError> {
    let status = resp.status();
    if (200..300).contains(&status) {
        if status == 204 { return serde_json::from_str("null").map_err(ApiError::from); }
        let text = resp.text().await.map_err(|e| ApiError::Network(e.to_string()))?;
        if text.trim().is_empty() { return Err(ApiError::Unknown(status, "Expected a JSON response but received an empty body.".into())); }
        serde_json::from_str(&text).map_err(ApiError::from)
    } else {
        let retry_after = parse_retry_after(resp.headers().get("retry-after").ok().flatten().as_deref());
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text, retry_after))
    }
}

pub async fn get_json<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::get(&url).credentials(RequestCredentials::Include).send().await?;
    handle_response(resp).await
}

pub async fn post_json<T: DeserializeOwned, B: Serialize>(path: &str, body: &B) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url).header("Content-Type", "application/json").credentials(RequestCredentials::Include)
        .json(body).map_err(|e| ApiError::Validation(e.to_string()))?.send().await?;
    handle_response(resp).await
}

pub async fn put_json<T: DeserializeOwned, B: Serialize>(path: &str, body: &B) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::put(&url).header("Content-Type", "application/json").credentials(RequestCredentials::Include)
        .json(body).map_err(|e| ApiError::Validation(e.to_string()))?.send().await?;
    handle_response(resp).await
}

async fn handle_no_content_response(resp: gloo_net::http::Response) -> Result<(), ApiError> {
    let status = resp.status();
    if (200..300).contains(&status) { return Ok(()); }
    let retry_after = parse_retry_after(resp.headers().get("retry-after").ok().flatten().as_deref());
    let text = resp.text().await.unwrap_or_default();
    Err(map_status(status, text, retry_after))
}

pub async fn post_json_no_content(path: &str) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url).credentials(RequestCredentials::Include).send().await?;
    handle_no_content_response(resp).await
}

pub async fn delete_json_no_content(path: &str) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::delete(&url).credentials(RequestCredentials::Include).send().await?;
    handle_no_content_response(resp).await
}

pub async fn get_json_query<T: DeserializeOwned>(path: &str, query: &[(&str, &str)]) -> Result<T, ApiError> {
    let mut url = format!("{API_BASE}{path}");
    if !query.is_empty() {
        let encoded = query.iter().map(|(k, v)| {
            let key = js_sys::encode_uri_component(k).as_string().unwrap_or_default();
            let value = js_sys::encode_uri_component(v).as_string().unwrap_or_default();
            format!("{key}={value}")
        }).collect::<Vec<_>>().join("&");
        url.push('?'); url.push_str(&encoded);
    }
    let resp = Request::get(&url).credentials(RequestCredentials::Include).send().await?;
    handle_response(resp).await
}

pub async fn delete_json<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::delete(&url).credentials(RequestCredentials::Include).send().await?;
    handle_response(resp).await
}

pub async fn put_json_no_content<B: Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::put(&url).header("Content-Type", "application/json").credentials(RequestCredentials::Include)
        .json(body).map_err(|e| ApiError::Validation(e.to_string()))?.send().await?;
    handle_no_content_response(resp).await
}

pub async fn patch_json_no_content<B: Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::patch(&url).header("Content-Type", "application/json").credentials(RequestCredentials::Include)
        .json(body).map_err(|e| ApiError::Validation(e.to_string()))?.send().await?;
    handle_no_content_response(resp).await
}

pub async fn post_multipart_file<T: DeserializeOwned>(path: &str, field_name: &str, file_name: &str, mime: &str, bytes: Vec<u8>) -> Result<T, ApiError> {
    let form = FormData::new().map_err(|_| ApiError::Validation("could not create upload form".into()))?;
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let blob_parts = js_sys::Array::new(); blob_parts.push(&array);
    let options = web_sys::BlobPropertyBag::new(); options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &options)
        .map_err(|_| ApiError::Validation("could not create upload payload".into()))?;
    form.append_with_blob_and_filename(field_name, &blob, file_name)
        .map_err(|_| ApiError::Validation("could not attach upload".into()))?;
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url).credentials(RequestCredentials::Include).body(form)
        .map_err(|e| ApiError::Validation(e.to_string()))?.send().await?;
    handle_response(resp).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_conflict_and_rate_limit() {
        assert!(matches!(map_status(409, r#"{"message":"already exists"}"#.into(), None), ApiError::Conflict(_)));
        assert!(matches!(map_status(429, r#"{"message":"slow down"}"#.into(), Some(12)), ApiError::RateLimited { retry_after: Some(12), .. }));
    }
    #[test]
    fn sanitizes_server_error_body() {
        let err = map_status(500, r#"{"message":"relation monitors does not exist"}"#.into(), None);
        assert_eq!(err.to_string(), "Server error (500): The server could not complete the request.");
    }
}

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "apps/leptos-dashboard/src/auth/api.rs")"
cat > "apps/leptos-dashboard/src/auth/api.rs" <<'PINGVERTEX_PHASE1_EOF'
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

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "apps/leptos-dashboard/src/components/require_auth.rs")"
cat > "apps/leptos-dashboard/src/components/require_auth.rs" <<'PINGVERTEX_PHASE1_EOF'
use crate::auth::api;
use crate::auth::context::{use_auth, AuthState};
use crate::components::layout::Layout;
use crate::components::loading::LoadingSpinner;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::Redirect;

#[component]
pub fn RequireAuth() -> impl IntoView {
    let auth = use_auth();
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    let check_session = move || {
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match api::fetch_current_user_typed().await {
                Ok(user) => auth.state.set(AuthState::Authenticated(user)),
                Err(message) => {
                    // Only a real 401 is allowed to transition the global auth
                    // state to logged-out. Infrastructure failures remain retryable.
                    if matches!(message, crate::api::client::ApiError::Unauthorized) {
                        auth.state.set(AuthState::Unauthenticated);
                    } else {
                        error.set(Some(if matches!(message, crate::api::client::ApiError::Network(_)) {
                            "Unable to reach PingVertex. Check your connection and try again.".to_string()
                        } else {
                            "We could not verify your session right now. Please try again.".to_string()
                        }));
                    }
                }
            }
            loading.set(false);
        });
    };

    Effect::new(move |_| check_session());

    view! {
        {move || {
            if loading.get() {
                view! { <div class="auth-loading"><LoadingSpinner text="Checking your session..."/></div> }.into_any()
            } else if let Some(message) = error.get() {
                view! {
                    <div class="auth-loading">
                        <div class="error-state">
                            <h2>"Session check failed"</h2>
                            <p>{message}</p>
                            <button class="btn btn-primary" on:click=move |_| check_session()>"Try again"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                match auth.state.get() {
                    AuthState::Authenticated(_) => view! { <Layout/> }.into_any(),
                    AuthState::Unauthenticated => view! { <Redirect path="/login"/> }.into_any(),
                    _ => view! { <div class="auth-loading"><LoadingSpinner text="Checking your session..."/></div> }.into_any(),
                }
            }
        }}
    }
}

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/dashboard_overview.rs")"
cat > "apps/leptos-dashboard/src/pages/dashboard_overview.rs" <<'PINGVERTEX_PHASE1_EOF'
//! Dashboard overview page backed by the single aggregated dashboard contract.

use crate::api::monitoring::{self, DashboardOverview, MonitorOverviewItem};
use crate::components::error_state::ErrorState;
use crate::components::loading::LoadingSpinner;
use crate::components::metric_card::MetricCard;
use crate::components::page_header::{PageHeader, PageHeaderProps};
use crate::components::status_badge::StatusBadge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

const RECENT_MONITORS_LIMIT: usize = 5;

fn user_facing_error(message: &str) -> String {
    if message.starts_with("Network error:") {
        "Unable to reach PingVertex. Check your connection and try again.".to_string()
    } else if message.starts_with("Server error") {
        "PingVertex could not load the dashboard right now. Please try again.".to_string()
    } else {
        message.to_string()
    }
}

#[component]
pub fn DashboardOverviewPage() -> impl IntoView {
    let overview = RwSignal::new(None::<DashboardOverview>);
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    let load_data = move || {
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match monitoring::fetch_overview().await {
                Ok(data) => overview.set(Some(data)),
                Err(e) => error.set(Some(user_facing_error(&e))),
            }
            loading.set(false);
        });
    };

    Effect::new(move |_| load_data());

    let on_retry = Callback::new(move |_: ()| load_data());
    let refresh_click = move |_| load_data();

    let page_header = PageHeader(
        PageHeaderProps::builder()
            .title("Overview".to_string())
            .children(Box::new(move || {
                view! { <button class="btn" on:click=refresh_click>"Refresh"</button> }.into_any()
            }))
            .build(),
    );

    view! {
        <div class="dashboard-overview">
            {page_header}
            {move || {
                if loading.get() {
                    view! { <LoadingSpinner text="Loading overview..."/> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <ErrorState title="Failed to load dashboard" description=err on_retry=on_retry/> }.into_any()
                } else if let Some(data) = overview.get() {
                    let summary = data.monitor_summary;
                    let recent_monitors: Vec<MonitorOverviewItem> = data.monitors.into_iter().take(RECENT_MONITORS_LIMIT).collect();
                    let open_incidents = data.incident_summary.open_count;
                    view! {
                        <div class="dashboard-content">
                            <div class="metric-strip">
                                <MetricCard label="Monitors" value=summary.total.to_string()/>
                                <MetricCard label="Up" value=summary.up.to_string()/>
                                <MetricCard label="Down" value=summary.down.to_string()/>
                                <MetricCard label="Disabled" value=summary.disabled.to_string()/>
                                <MetricCard label="Active Incidents" value=open_incidents.to_string()/>
                            </div>
                            <div class="dashboard-widgets">
                                <section class="dashboard-widget monitor-status-widget">
                                    <h2>"Monitor Status"</h2>
                                    {if recent_monitors.is_empty() {
                                        view! { <p class="widget-empty">"No monitors yet."</p> }.into_any()
                                    } else {
                                        view! {
                                            <ul class="monitor-status-list">
                                                {recent_monitors.into_iter().map(|m| {
                                                    let status = m.status.unwrap_or_else(|| if m.enabled { "unknown".to_string() } else { "disabled".to_string() });
                                                    view! {
                                                        <li class="monitor-status-item">
                                                            <span class="monitor-status-name">{m.name}</span>
                                                            <StatusBadge status=status/>
                                                        </li>
                                                    }
                                                }).collect_view()}
                                            </ul>
                                        }.into_any()
                                    }}
                                </section>
                                <section class="dashboard-widget active-incidents-widget">
                                    <h2>"Active Incidents"</h2>
                                    <p class="active-incidents-count">{open_incidents}</p>
                                    <A href="/monitors">"View monitors"</A>
                                </section>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <p class="widget-empty">"No dashboard data available."</p> }.into_any()
                }
            }}
        </div>
    }
}

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "crates/infra/src/repositories/postgres_dashboard_repository.rs")"
cat > "crates/infra/src/repositories/postgres_dashboard_repository.rs" <<'PINGVERTEX_PHASE1_EOF'
use std::error::Error;

use application::{
    dto::{
        DashboardOverview, IncidentSummary, IncidentSummaryData, MonitorOverviewItem,
        MonitorSummary,
    },
    ports::DashboardRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::CheckStatus;
use domain::shared::ids::TenantId;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresDashboardRepository {
    pool: PgPool,
}

impl PostgresDashboardRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct MonitorWithLatest {
    id: Uuid,
    name: String,
    url: String,
    enabled: bool,
    latest_status: Option<i16>,
    latest_latency_ms: Option<i64>,
    latest_checked_at: Option<DateTime<Utc>>,
    open_incident_id: Option<Uuid>,
    open_incident_started_at: Option<DateTime<Utc>>,
    open_incident_acknowledged_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct RecentIncidentRow {
    id: Uuid,
    monitor_id: Uuid,
    started_at: DateTime<Utc>,
    acknowledged_at: Option<DateTime<Utc>>,
}

#[async_trait]
impl DashboardRepository for PostgresDashboardRepository {
    async fn get_overview(&self, tenant_id: TenantId) -> RepoResult<DashboardOverview> {
        // Query 1: all monitors with their latest check and open incident
        // (if any) using two LATERAL joins. This replaces the naive N+1
        // pattern of fetching each monitor's checks/incidents individually.
        let monitor_rows = sqlx::query_as::<_, MonitorWithLatest>(
            r#"
            SELECT
                m.id,
                m.name,
                m.url,
                m.enabled,
                lc.status AS latest_status,
                lc.response_time_ms AS latest_latency_ms,
                lc.checked_at AS latest_checked_at,
                oi.id AS open_incident_id,
                oi.started_at AS open_incident_started_at,
                oi.acknowledged_at AS open_incident_acknowledged_at
            FROM monitors m
            LEFT JOIN LATERAL (
                SELECT status, response_time_ms, checked_at
                FROM checks c
                WHERE c.monitor_id = m.id
                ORDER BY c.checked_at DESC
                LIMIT 1
            ) lc ON true
            LEFT JOIN LATERAL (
                SELECT id, started_at, acknowledged_at
                FROM incidents i
                WHERE i.monitor_id = m.id
                  AND i.resolved_at IS NULL
                ORDER BY i.started_at DESC
                LIMIT 1
            ) oi ON true
            WHERE m.tenant_id = $1
            ORDER BY m.created_at DESC
            "#,
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        // Query 2: 5 most recent incidents (resolved or open) for the tenant.
        let recent_incidents = sqlx::query_as::<_, RecentIncidentRow>(
            r#"
            SELECT
                i.id,
                i.monitor_id,
                i.started_at,
                i.acknowledged_at
            FROM incidents i
            JOIN monitors m ON i.monitor_id = m.id
            WHERE m.tenant_id = $1
            ORDER BY i.started_at DESC
            LIMIT 5
            "#,
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        // Aggregate into response DTO
        let mut monitors = Vec::with_capacity(monitor_rows.len());
        let mut up_count = 0u32;
        let mut down_count = 0u32;
        let mut disabled_count = 0u32;
        let mut open_count = 0u32;

        for row in monitor_rows {
            let status = row.latest_status.map(|s| {
                if s == 1 {
                    CheckStatus::Up
                } else {
                    CheckStatus::Down
                }
            });

            if !row.enabled {
                disabled_count += 1;
            } else if let Some(status) = status {
                match status {
                    CheckStatus::Up => up_count += 1,
                    CheckStatus::Down => down_count += 1,
                }
            }

            if row.open_incident_id.is_some() {
                open_count += 1;
            }

            monitors.push(MonitorOverviewItem {
                id: row.id.to_string(),
                name: row.name,
                url: row.url,
                enabled: row.enabled,
                status,
                latency_ms: row.latest_latency_ms.map(|v| v as u128),
                checked_at: row.latest_checked_at,
                open_incident: row.open_incident_id.map(|incident_id| IncidentSummary {
                    id: incident_id.to_string(),
                    monitor_id: row.id.to_string(),
                    started_at: row.open_incident_started_at.unwrap_or_else(Utc::now),
                    acknowledged_at: row.open_incident_acknowledged_at,
                }),
            });
        }

        let total = monitors.len() as u32;

        let monitor_summary = MonitorSummary {
            total,
            up: up_count,
            down: down_count,
            disabled: disabled_count,
        };

        let incident_summary = IncidentSummaryData {
            open_count,
            recent: recent_incidents
                .into_iter()
                .map(|r| IncidentSummary {
                    id: r.id.to_string(),
                    monitor_id: r.monitor_id.to_string(),
                    started_at: r.started_at,
                    acknowledged_at: r.acknowledged_at,
                })
                .collect(),
        };

        Ok(DashboardOverview {
            monitor_summary,
            monitors,
            incident_summary,
        })
    }
}

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "crates/infra/tests/postgres_dashboard_repository.rs")"
cat > "crates/infra/tests/postgres_dashboard_repository.rs" <<'PINGVERTEX_PHASE1_EOF'
use application::ports::DashboardRepository;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId, UserId};
use domain::{
    Check, CheckInterval, CheckStatus, ErrorType, ExpectedStatus, HttpMethod, Incident,
    MonitorBuilder, MonitorName, RegionCode, Timeout, Url,
};
use infra::PostgresDashboardRepository;
use sqlx::PgPool;
use uuid::Uuid;

mod common;

async fn insert_user_and_tenant(pool: &PgPool, user_id: UserId) -> TenantId {
    let email = format!("{}@example.test", user_id.0);
    let tenant_id = TenantId::new();
    sqlx::query("INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at) VALUES ($1,$2,$3,$4,NOW(),NOW())")
        .bind(user_id.0).bind(email).bind("not-a-real-hash").bind("Test User")
        .execute(pool).await.unwrap();
    sqlx::query(
        "INSERT INTO tenants (id, name, owner_user_id, created_at) VALUES ($1,$2,$3,NOW())",
    )
    .bind(tenant_id.0)
    .bind(format!("Tenant {}", user_id.0))
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role, invited_at, joined_at) VALUES ($1,$2,'Owner',NOW(),NOW())")
        .bind(tenant_id.0).bind(user_id.0)
        .execute(pool).await.unwrap();
    tenant_id
}

async fn insert_monitor_with_data(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: TenantId,
    user_id: UserId,
    name: &str,
) {
    let monitor = MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id, tenant_id)
        .name(MonitorName::new(name).unwrap())
        .url(Url::new("https://example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build();

    sqlx::query(
        r#"
        INSERT INTO monitors (id, user_id, tenant_id, regions, name, url, method, expected_status,
            interval_seconds, timeout_seconds, enabled, is_public, ssl_check_enabled, ssl_expiry_warning_days,
            kind, created_at, updated_at)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
        "#,
    )
    .bind(monitor.id.0)
    .bind(monitor.user_id.0)
    .bind(monitor.tenant_id.0)
    .bind(Vec::<String>::new())
    .bind(monitor.name.as_str())
    .bind(monitor.url.clone())
    .bind("GET")
    .bind(200i16)
    .bind(60i32)
    .bind(10i32)
    .bind(true)
    .bind(false)
    .bind(true)
    .bind(14i32)
    .bind("http")
    .bind(monitor.created_at)
    .bind(monitor.updated_at)
    .execute(&mut **tx)
    .await
    .unwrap();

    let check = Check::new(
        CheckId(Uuid::new_v4()),
        monitor.id,
        RegionCode::default_region(),
        CheckStatus::Up,
        Some(200),
        100,
        ErrorType::None,
        None,
    );
    sqlx::query("INSERT INTO checks (id, monitor_id, region, status, http_status, response_time_ms, error_type, checked_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(check.id.0).bind(check.monitor_id.0).bind(check.region.to_string())
        .bind(1i16).bind(check.status_code.map(i32::from)).bind(check.latency_ms as i64)
        .bind("None").bind(check.checked_at)
        .execute(&mut **tx)
        .await
        .unwrap();

    let incident = Incident::new(IncidentId(Uuid::new_v4()), monitor.id);
    sqlx::query("INSERT INTO incidents (id, monitor_id, started_at, resolved_at, last_seen_at, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(incident.id.0).bind(incident.monitor_id.0).bind(incident.started_at)
        .bind(incident.resolved_at).bind(incident.last_seen_at).bind(incident.created_at).bind(incident.updated_at)
        .execute(&mut **tx)
        .await
        .unwrap();
}

#[tokio::test]
#[serial_test::serial]
async fn get_overview_handles_1000_monitors_without_n_plus_one_queries() {
    let pool = common::setup_pool().await;
    let repo = PostgresDashboardRepository::new(pool.clone());

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = insert_user_and_tenant(&pool, user_id).await;

    // Use a single transaction for all 1000 inserts – atomic and fast.
    let mut tx = pool.begin().await.expect("begin transaction");
    let start = std::time::Instant::now();
    for i in 0..1000 {
        let name = format!("monitor-{i}");
        insert_monitor_with_data(&mut tx, tenant_id, user_id, &name).await;
    }
    tx.commit().await.expect("commit transaction");
    let insertion_elapsed = start.elapsed();
    println!("Inserted 1000 monitors in {:?}", insertion_elapsed);

    // Optional diagnostic: confirm count in DB (remove after debugging if desired)
    let db_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM monitors WHERE tenant_id = $1")
        .bind(tenant_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    println!("DB monitor count: {}", db_count);
    assert_eq!(db_count, 1000, "Monitors were not all inserted");

    let query_start = std::time::Instant::now();
    let overview = repo
        .get_overview(tenant_id)
        .await
        .expect("get_overview should succeed");
    let query_elapsed = query_start.elapsed();
    println!("Fetched overview in {:?}", query_elapsed);

    assert_eq!(overview.monitor_summary.total, 1000);
    assert_eq!(overview.monitor_summary.up, 1000);
    assert_eq!(overview.monitor_summary.down, 0);
    assert_eq!(overview.monitor_summary.disabled, 0);
    assert_eq!(overview.monitors.len(), 1000);
    assert_eq!(overview.incident_summary.open_count, 1000);
    assert_eq!(overview.incident_summary.recent.len(), 5);

    assert!(
        query_elapsed.as_secs_f64() < 2.0,
        "dashboard overview query took too long: {:?} — likely N+1",
        query_elapsed
    );
}


#[tokio::test]
#[serial_test::serial]
async fn get_overview_counts_disabled_monitors_separately_from_up_down() {
    let pool = common::setup_pool().await;
    let repo = PostgresDashboardRepository::new(pool.clone());

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = insert_user_and_tenant(&pool, user_id).await;

    let mut tx = pool.begin().await.unwrap();
    insert_monitor_with_data(&mut tx, tenant_id, user_id, "enabled").await;
    insert_monitor_with_data(&mut tx, tenant_id, user_id, "disabled").await;
    tx.commit().await.unwrap();

    sqlx::query("UPDATE monitors SET enabled = FALSE WHERE tenant_id = $1 AND name = 'disabled'")
        .bind(tenant_id.0)
        .execute(&pool)
        .await
        .unwrap();

    let overview = repo.get_overview(tenant_id).await.unwrap();
    assert_eq!(overview.monitor_summary.total, 2);
    assert_eq!(overview.monitor_summary.up, 1);
    assert_eq!(overview.monitor_summary.down, 0);
    assert_eq!(overview.monitor_summary.disabled, 1);
}

#[tokio::test]
#[serial_test::serial]
async fn get_overview_respects_tenant_isolation() {
    let pool = common::setup_pool().await;
    let repo = PostgresDashboardRepository::new(pool.clone());

    let user_a = UserId(Uuid::new_v4());
    let tenant_a = insert_user_and_tenant(&pool, user_a).await;
    let user_b = UserId(Uuid::new_v4());
    let tenant_b = insert_user_and_tenant(&pool, user_b).await;

    let mut tx = pool.begin().await.unwrap();
    insert_monitor_with_data(&mut tx, tenant_a, user_a, "tenant-a-monitor").await;
    tx.commit().await.unwrap();

    let overview_a = repo.get_overview(tenant_a).await.unwrap();
    assert_eq!(overview_a.monitor_summary.total, 1);
    assert_eq!(overview_a.monitors.len(), 1);

    let overview_b = repo.get_overview(tenant_b).await.unwrap();
    assert_eq!(overview_b.monitor_summary.total, 0);
    assert_eq!(overview_b.monitors.len(), 0);
    assert_eq!(overview_b.incident_summary.open_count, 0);
}

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "docs/FRONTEND_BACKEND_CONTRACT_AUDIT_2026-09-03.md")"
cat > "docs/FRONTEND_BACKEND_CONTRACT_AUDIT_2026-09-03.md" <<'PINGVERTEX_PHASE1_EOF'
# PingVertex — Frontend ↔ Backend Contract Audit (2026-09-03)

## Source-of-truth trace

`dashboard_overview.rs → api/monitoring.rs → /api/v1/dashboard/overview → crates/api/src/routes/dashboard.rs → GetDashboardOverviewUseCase → DashboardRepository → PostgresDashboardRepository → SQL/migrations → JSON → frontend`

## DashboardOverview result

| Backend | Frontend | Result |
|---|---|---|
| `monitor_summary.total/up/down/disabled` | same | ALIGNED |
| `monitors[]` | same | ALIGNED |
| `incident_summary.open_count` | same | ALIGNED |
| `incident_summary.recent[]` | same | ALIGNED |

The old frontend summary (`total_monitors`, `uptime_percentage`, `average_response_time_ms`, `active_incidents`) was removed. The dashboard now performs one aggregated request and consumes the backend aggregation directly.

## Security / performance findings applied

- Dashboard route is JWT-protected in `crates/api/src/router.rs`.
- Repository scopes monitor data by `TenantId` and incident data through tenant-owned monitors.
- LATERAL joins provide latest check/open incident without a client-side N+1 loop.
- Existing indexes cover tenant monitor lookup and monitor/check/incident lookups.
- Disabled monitors no longer inflate `up`/`down`; they are counted only in `disabled`.
- Frontend 5xx messages are sanitized.
- Frontend explicitly maps 409 and 429; numeric `Retry-After` is retained.
- Structured `{message}` API errors are parsed.
- Authentication redirects happen only on an actual 401; network/server failures remain retryable.
- No unsafe automatic mutation retries were introduced.

## Runtime verification still required

This source snapshot cannot run against the user's local PostgreSQL/Redis/browser environment. After installation run:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/leptos-dashboard && trunk build --release
cd ../..
```

## Frontend API inventory

- `advanced_monitoring.rs::list_maintenance` → /escalation-po, /maintenance-windows/, /monitors/
- `advanced_monitoring.rs::create_maintenance` → /escalation-policies, /maintenance-windows/, /monitors/
- `advanced_monitoring.rs::delete_maintenance` → /escalation-policies, /escalation-policies/, /maintenance-windows/, /monitors/
- `advanced_monitoring.rs::list_escalations` → /escalation-policies, /escalation-policies/, /maintenance-windows/, /monitors/
- `advanced_monitoring.rs::create_escalation` → /escalation-policies, /escalation-policies/, /maintenance-windows/
- `advanced_monitoring.rs::update_escalation` → /escalation-policies, /escalation-policies/
- `advanced_monitoring.rs::delete_escalation` → /escalation-policies, /escalation-policies/
- `billing.rs::summary` → /billing/addons, /billing/checkout, /billing/checkout/verify, /billing/summary, /pricing/quote
- `billing.rs::quote` → /billing/addons, /billing/checkout, /billing/checkout/verify, /billing/summary, /pricing/quote
- `billing.rs::checkout` → /billing/addons, /billing/checkout, /billing/checkout/verify, /billing/summary, /pricing/quote
- `billing.rs::verify_checkout` → /billing/addons, /billing/checkout, /billing/checkout/verify, /billing/summary, /pricing/quote
- `billing.rs::add_on` → /billing/addons, /billing/checkout, /billing/checkout/verify
- `developer.rs::list_keys` → /api-keys, /api-keys/, /monitors/
- `developer.rs::create_key` → /api-keys, /api-keys/, /monitors/
- `developer.rs::delete_key` → /api-keys, /api-keys/, /monitors/
- `developer.rs::list_flows` → /api-keys, /api-keys/, /monitors/
- `developer.rs::create_flow` → /api-keys, /api-keys/, /monitors/
- `developer.rs::update_flow` → /api-keys/, /monitors/
- `developer.rs::delete_flow` → /monitors/
- `monitoring.rs::fetch_monitors` → /monitors, /monitors/
- `monitoring.rs::get_monitor` → /monitors, /monitors/
- `monitoring.rs::create_monitor` → /das, /monitors, /monitors/
- `monitoring.rs::update_monitor` → /dashboard/overview, /monitors, /monitors/
- `monitoring.rs::delete_monitor` → /dashboard/overview, /monitors, /monitors/
- `monitoring.rs::set_monitor_enabled` → /dashboard/overview, /monitors, /monitors/
- `monitoring.rs::fetch_overview` → /dashboard/overview, /monitors/
- `monitoring.rs::fetch_incidents` → /dashboard/overview, /monitors/
- `monitoring.rs::fetch_checks` → /dashboard/overview, /monitors/, /notifications/
- `monitoring.rs::list_notification_channels` → /dashboard/overview, /monitors/, /notifications/
- `monitoring.rs::create_notification_channel` → /monitors/, /notifications/
- `monitoring.rs::delete_notification_channel` → /escalation-policies, /monitors/, /notifications/
- `monitoring.rs::test_notification_channel` → /escalation-policies, /incidents/, /monitors/, /notifications/
- `monitoring.rs::create_maintenance_window` → /escalation-policies, /incidents/, /monitors/, /notifications/
- `monitoring.rs::create_escalation_policy` → /escalation-policies, /incidents/, /monitors/, /notifications/
- `monitoring.rs::acknowledge_incident` → /escalation-policies, /incidents/, /monitors/
- `rum.rs::list_rum_sites_typed` → /dashboard/rum/sites, /dashboard/rum/sites/
- `rum.rs::create_rum_site_typed` → /dashboard/rum/sites, /dashboard/rum/sites/
- `rum.rs::rum_overview_typed` → /dashboard/rum/sites, /dashboard/rum/sites/
- `rum.rs::rum_slow_pages_typed` → /dashboard/rum/sites/
- `rum.rs::rum_error_breakdown_typed` → /dashboard/rum/sites/
- `rum.rs::rum_browser_device_breakdown_typed` → /dashboard/rum/sites/
- `rum.rs::rum_geo_breakdown_typed` → /dashboard/rum/sites/
- `rum.rs::rum_performance_trend_typed` → /dashboard/rum/sites/
- `rum.rs::rum_session_stats_typed` → /dashboard/rum/sites/
- `status_pages.rs::list` → /st, /status-pages, /status-pages/
- `status_pages.rs::create` → /status-pages, /status-pages/
- `status_pages.rs::update` → /status-pages, /status-pages/
- `status_pages.rs::delete` → /monitors, /status-pages, /status-pages/
- `status_pages.rs::request_custom_domain` → /monitors, /status-pages, /status-pages/
- `status_pages.rs::remove_custom_domain` → /monitors, /status-pages/
- `status_pages.rs::upload_logo` → /monitors, /status-pages/
- `status_pages.rs::monitors` → /monitors, /status-pages/
- `support.rs::categories` → /help/articles, /help/articles/, /help/categories, /help/search, /support/tickets, /support/tickets/
- `support.rs::articles` → /help/articles, /help/articles/, /help/categories, /help/search, /support/tickets, /support/tickets/
- `support.rs::article` → /help/articles, /help/articles/, /help/categories, /help/search, /support/tickets, /support/tickets/
- `support.rs::search` → /help/articles, /help/articles/, /help/search, /support/tickets, /support/tickets/
- `support.rs::tickets` → /help/articles, /help/articles/, /help/search, /support/tickets, /support/tickets/
- `support.rs::ticket` → /help/articles, /help/articles/, /help/search, /support/tickets, /support/tickets/
- `support.rs::create_ticket` → /help/articles/, /help/search, /support/tickets, /support/tickets/
- `support.rs::reply` → /help/search, /support/tickets, /support/tickets/
- `support.rs::set_status` → /support/tickets, /support/tickets/
- `team.rs::list_members` → /invitations, /invitations/pending, /members, /members/
- `team.rs::add_member` → /invitations, /invitations/, /invitations/pending, /members, /members/
- `team.rs::change_role` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members, /members/
- `team.rs::remove_member` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members, /members/
- `team.rs::list_invitations` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members, /members/
- `team.rs::create_invitation` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members/
- `team.rs::revoke_invitation` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members/
- `team.rs::accept_invitation` → /invitations, /invitations/, /invitations/accept, /invitations/pending, /members/

## Backend route inventory

- `api_flows.rs` → `/monitors/{id}/flows`
- `api_flows.rs` → `/monitors/{id}/flows/{flow_id}`
- `api_keys.rs` → `/api-keys`
- `api_keys.rs` → `/api-keys/{id}`
- `api_keys.rs` → `/external/monitors`
- `api_keys.rs` → `/external/checks`
- `api_keys.rs` → `/external/incidents/{id}`
- `api_keys.rs` → `/external/incidents/{incident_id}/acknowledge`
- `api_keys.rs` → `/external/status-pages`
- `api_keys.rs` → `/external/status-pages/{id}`
- `auth.rs` → `/register`
- `auth.rs` → `/login`
- `auth.rs` → `/oauth/google`
- `auth.rs` → `/oauth/apple`
- `auth.rs` → `/verify-email`
- `auth.rs` → `/resend-verification`
- `auth.rs` → `/forgot-password`
- `auth.rs` → `/reset-password`
- `auth.rs` → `/totp/verify`
- `auth.rs` → `/logout`
- `auth.rs` → `/me`
- `auth.rs` → `/totp/enable`
- `auth.rs` → `/totp/confirm`
- `auth.rs` → `/totp/disable`
- `auth.rs` → `/password/change`
- `billing.rs` → `/billing/checkout`
- `billing.rs` → `/billing/checkout/verify`
- `billing.rs` → `/billing/addons`
- `billing.rs` → `/billing/summary`
- `billing.rs` → `/billing/webhook`
- `checks.rs` → `/checks`
- `dashboard.rs` → `/dashboard/overview`
- `health.rs` → `/health`
- `health.rs` → `/ready`
- `help.rs` → `/help/categories`
- `help.rs` → `/help/articles`
- `help.rs` → `/help/articles/{slug}`
- `help.rs` → `/help/search`
- `internal_agents.rs` → `/agents/{agent_id}/checks/claim`
- `internal_agents.rs` → `/agents/{agent_id}/results`
- `invitations.rs` → `/invitations`
- `invitations.rs` → `/invitations/pending`
- `invitations.rs` → `/invitations/accept`
- `invitations.rs` → `/invitations/{id}`
- `members.rs` → `/members`
- `members.rs` → `/members/{user_id}`
- `members.rs` → `/members/{user_id}/role`
- `monitors.rs` → `/monitors`
- `monitors.rs` → `/monitors/{id}`
- `monitors.rs` → `/monitors/{id}/enable`
- `monitors.rs` → `/monitors/{id}/disable`
- `monitors.rs` → `/monitors/{id}/checks`
- `monitors.rs` → `/monitors/{id}/incidents`
- `monitors.rs` → `/monitors/{id}/maintenance-windows`
- `monitors.rs` → `/escalation-policies`
- `monitors.rs` → `/escalation-policies/{id}`
- `monitors.rs` → `/incidents/{incident_id}/acknowledge`
- `notifications.rs` → `/monitors/{monitor_id}/notifications`
- `notifications.rs` → `/notifications/{id}`
- `notifications.rs` → `/notifications/{id}/test`
- `pricing.rs` → `/pricing/quote`
- `pricing.rs` → `/pricing/configs/active`
- `pricing.rs` → `/pricing/configs`
- `pricing.rs` → `/pricing/configs/{version}`
- `pricing.rs` → `/pricing/audit-logs`
- `proxy_checks.rs` → `/proxy-checks`
- `proxy_checks.rs` → `/proxy-checks/{id}`
- `public.rs` → `/status`
- `public.rs` → `/status/{subdomain}`
- `public.rs` → `/heartbeat/{token}`
- `public.rs` → `/rum/collect`
- `rum_dashboard.rs` → `/dashboard/rum/sites`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/overview`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/pages`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/errors`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/browsers-devices`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/geo`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/trend`
- `rum_dashboard.rs` → `/dashboard/rum/sites/{site_id}/sessions`
- `status_pages.rs` → `/status-pages`
- `status_pages.rs` → `/status-pages/{id}`
- `status_pages.rs` → `/status-pages/{id}/logo`
- `status_pages.rs` → `/status-pages/{id}/custom-domain`
- `subscription.rs` → `/subscription/usage`
- `support.rs` → `/support/tickets`
- `support.rs` → `/support/tickets/{id}`
- `support.rs` → `/support/tickets/{id}/messages`
- `support.rs` → `/support/tickets/{id}/status`

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "scripts/audit_frontend_backend_contracts.sh")"
cat > "scripts/audit_frontend_backend_contracts.sh" <<'PINGVERTEX_PHASE1_EOF'
#!/usr/bin/env bash
set -euo pipefail
ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$ROOT"
echo "== Dashboard contract =="
grep -nE 'DashboardOverview|monitor_summary|incident_summary|/dashboard/overview' apps/leptos-dashboard/src/api/monitoring.rs apps/leptos-dashboard/src/pages/dashboard_overview.rs crates/application/src/dto/dashboard_overview.rs crates/api/src/routes/dashboard.rs
echo
echo "== Protected dashboard route =="
grep -n -B2 -A3 'dashboard::router' crates/api/src/router.rs
echo
echo "== Performance indexes =="
grep -RInE 'idx_monitors_tenant_id|idx_checks_monitor_checked_at|idx_incidents_monitor_resolved_at|idx_incidents_monitor_started_at' migrations
echo
echo "== Frontend API functions =="
grep -RInE '^pub async fn ' apps/leptos-dashboard/src/api | sort
echo
echo "== Backend route declarations =="
grep -RInE '\.route\(' crates/api/src/routes | sort

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "HANDOVER.md")"
cat > "HANDOVER.md" <<'PINGVERTEX_PHASE1_EOF'
# PingVertex Leptos Frontend Migration — Handover

## Worker: L9 — CI Workflow for Leptos Frontend

### Scope
- Add CI workflow for Leptos frontend (apps/leptos-dashboard).
- Ensure frontend builds and tests run on every push/PR.
- Do not break existing backend CI.
- Follow existing patterns from H7/H10 backend CI.

### Status
COMPLETE — CI workflow added as a separate file; no existing jobs modified.

### Files Created/Modified/Deleted
- **Created** `.github/workflows/leptos-frontend.yml` — new workflow for frontend.
- **Updated** `HANDOVER.md` — this file.

### Frontend Changes
- None in application code; only CI infrastructure.

### Tests
Local verification commands (to be run in terminal):
- `cd apps/leptos-dashboard && trunk build --release`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test -p leptos-dashboard`

CI will run the same steps automatically on push/PR, but the test step is limited to the frontend crate to avoid requiring database services.

### Deviations from Prompt
- Used a **separate workflow file** (`.github/workflows/leptos-frontend.yml`) instead of modifying existing backend CI. This is safer and avoids accidental breakage; both workflows will run on their respective triggers.
- Added a conditional step for building Docker image (`Dockerfile.frontend`) only if that file exists (in case L7 did not create it).
- Did not include `cargo audit`/`cargo deny`/trivy for frontend; those are backend-specific. Frontend has no additional dependencies beyond Rust crates (already covered by clippy and tests). If needed later, they can be added.
- **Changed test step from `cargo test --workspace` to `cargo test -p leptos-dashboard`** to avoid running backend integration tests that require PostgreSQL. This keeps the frontend CI lightweight and self-contained.

### Known Issues
- None.

### Notes for Next Worker
- The frontend CI is independent; backend CI remains untouched.
- If L7 provides a `Dockerfile.frontend`, the conditional docker build step will activate automatically.
- Ensure `Cargo.lock` is committed so caching works properly.
- For local full workspace tests, you may need to set up PostgreSQL and Redis; but the frontend CI does not run those tests.

### Completion Criteria
- CI workflow file present and syntactically valid.
- All steps pass when run manually or via GitHub Actions.
- Backend CI jobs remain intact and functional.

## Worker: Audit Remediation — 12 Remaining Findings (2026-09-02)

### Scope
Close all 12 remaining findings from the codebase/database audit
(migrations + domain + application + infra + API + RUM), without
breaking any existing logic. See the full audit findings document for
the original issue numbering/severity used below.

### Status
COMPLETE for issues #1, #2, #4–#12. Issue #3 originally shipped as
documentation-only (project was pre-production/fresh-DB-only at the
time) — see its own section below for that background.
Follow-up (2026-09): a post-remediation review flagged that
`rum_js_errors`/`rum_resource_errors.page_view_id` was still a plain FK
(not tenant/site composite) — closed by `migrations/0072_rum_page_view_composite_fk.sql`.
Second follow-up (2026-09): a real (not documentation-only) upgrade path
for issue #3 was requested and added —
`scripts/migrate_existing_checks_to_partitioned.sh`. It has NOT been run
against a real database by whoever authored it; treat it as reviewed-but-
unverified until it's been dry-run and then exercised against a staging
copy of real data.

### Files Created/Modified
- **Modified** `crates/domain/src/repositories/check_repository.rs`,
  `crates/application/src/ports/check_repository.rs`,
  `crates/infra/src/repositories/postgres_check_repository.rs` — Issue
  #1: `CheckRepository::find_by_id` now takes `(id, checked_at)`, matching
  `checks`' actual composite PRIMARY KEY since migration 0061. No
  production call site existed for this method (verified by search), so
  this was a safe, mechanical signature change; only test mocks needed
  updating (24 files across `crates/api/tests`, `crates/application/tests`,
  `crates/infra/tests`, `crates/worker/src/scheduler.rs`).
- **Created** `migrations/0069_enforce_rum_composite_tenant_integrity.sql`
  — Issues #5/#6: composite `(site_id, tenant_id)` and
  `(session_id, tenant_id, site_id)` foreign keys across the RUM table
  hierarchy (`rum_sessions`, `rum_page_views`, `rum_js_errors`,
  `rum_resource_errors`), same pattern as migration 0065's monitor-child
  tenant integrity fix.
- **Created** `migrations/0070_fix_diagnostic_reports_composite_uniqueness.sql`
  — Issue #2: replaces the stale `UNIQUE(check_id)` index with
  `UNIQUE(check_id, check_checked_at)`, matching the composite identity
  migrations 0060/0061 already established.
- **Created** `migrations/0071_checks_partition_reconciliation_and_health.sql`
  — Issues #4/#10/#12: `ensure_checks_partition` redefined
  (`CREATE OR REPLACE`, same signature) to use a schema-qualified
  `to_regclass` lookup instead of unqualified `pg_class.relname`; new
  `reconcile_checks_default_partition`, `checks_partition_maintenance`,
  and `checks_partition_health` functions.
- **Modified** `scripts/ensure_checks_partitions.sh` — now calls
  `checks_partition_maintenance` (ensure + reconcile) instead of
  `ensure_checks_partition` alone, prints `checks_partition_health`
  output every run, and warns (non-fatally) if the systemd timer isn't
  active on hosts that have `systemctl`.
- **Created** `scripts/install_checks_partition_timer.sh` — Issue #11:
  idempotent installer for the systemd service+timer
  (`daemon-reload` + `enable --now`), meant to be wired into a deploy
  pipeline instead of relying on a human remembering the manual steps.
- **Modified** `crates/api/src/handlers/rum_ingest.rs` — Issues #7/#8:
  implements the previously-documented-but-missing Origin/Referer vs
  `RumSite.domain` allow-list check (advisory/logging only — Origin and
  Referer remain client-controlled and are not treated as a security
  boundary); adds a second per-`(site_key, IP)` rate limit under the
  existing per-site limit so a public site_key alone can't be used to
  exhaust a site's whole quota.
- **Modified** `crates/infra/src/repositories/postgres_check_repository.rs`
  — Issue #9: added `id DESC` as an explicit tie-breaker after
  `checked_at DESC` in every ordered/paginated query, since `checked_at`
  is not unique and offset pagination on a non-unique sort key can
  otherwise skip or duplicate rows across pages.

### Issue #3 — real upgrade-path tool added (2026-09, follow-up)
Originally resolved as documentation-only (see below), since this
project's status at the time was pre-production/fresh-DB-only. A
follow-up review requested a real, safe upgrade path regardless, so
`scripts/migrate_existing_checks_to_partitioned.sh` was added: a
manually-invoked (never auto-run), resumable, batched shadow-table
copy + verified atomic swap that converts an existing, populated
`checks` table into the exact partitioned structure migration 0061
creates, without needing 0061 itself to touch non-empty tables. Full
procedure, safety notes, and the required manual
`_sqlx_migrations` reconciliation step (sqlx has no built-in "mark
applied without running" command — the safe procedure is documented
in the script's own header) are in the script itself. This was NOT
added as a new numbered migration, and deliberately does not touch
`_sqlx_migrations` programmatically — both by design, since this is a
one-time, operator-supervised production data operation, not something
safe to run unattended as part of a routine deploy.

Original documentation-only resolution, still accurate as background:
Migration 0061's safety guard (it refuses to run against non-empty
`checks`/`diagnostic_reports` tables) was verified still intact and
untouched. Per the audit's own required-fix options, and since this
project has explicitly confirmed pre-production/fresh-DB-only status (see
0061's own header comment), the correct action is documentation, not a
schema change: **0061 is fresh-database-only and must never be run as an
upgrade migration against a database with existing `checks` history.** If
this project ever needs to upgrade a database that already has real
`checks`/`diagnostic_reports` rows, a new migration implementing a
dual-write/backfill/swap strategy must be written for that specific
upgrade — do not attempt to modify or re-run 0061 for that purpose.

### Tests
Could not be executed in the environment this remediation was authored
in (no `cargo`/toolchain available). Run before deploying:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` (requires a running Postgres — see
  `crates/infra/tests/common/mod.rs` for connection setup)
- Apply migrations 0069–0071 against a staging database (or a scratch
  copy of production) **before** applying to any environment with real
  data — 0069 in particular will fail loudly (by design) if any existing
  RUM rows are already cross-tenant-inconsistent, and 0071's
  `reconcile_checks_default_partition` moves live rows between tables.

### Deviations from Prompt
- Did not edit any existing migration file in place (0061, 0068, etc.) —
  followed this repo's own established convention (see migration 0063's
  comment: "migrations here are treated as append-only") and instead
  added new migrations, using `CREATE OR REPLACE FUNCTION` where a
  function's *definition* needed to change but its name/signature could
  stay stable for existing callers.
- Issue #1's "choose Option A or B" decision was made as **Option B**
  (composite `(CheckId, checked_at)` identity), since migrations
  0060/0061 had already moved the database itself to that model — Option
  A would have required an additional migration to add back a
  single-column uniqueness guarantee the partitioned table structure
  doesn't naturally provide.
- Issue #7 was implemented as **Option A** (implement the allow-list
  check) rather than Option B (remove the misleading docs), per the
  audit's own recommendation, and kept strictly advisory/logging-only per
  its own security nuance about Origin/Referer not being a strong
  boundary.

### Known Issues
- None identified beyond what's noted in the "Tests" section above
  (unverified against a real toolchain/database in this environment).

### Notes for Next Worker
- **DONE (2026-09, follow-up):** `rum_page_views.page_view_id`-level
  tenant/site composite FKs on `rum_js_errors`/`rum_resource_errors` were
  added in `migrations/0072_rum_page_view_composite_fk.sql`, closing the
  gap the note at the bottom of migration 0069 originally deferred.
- `checks_partition_health(months_ahead)` (migration 0071) is meant to be
  wired into whatever metrics/alerting this project uses next; it
  currently only surfaces via `ensure_checks_partitions.sh`'s log output.

## Worker: Phase 1 — Backend ↔ Frontend Integration Hardening (2026-09-03)

### Scope
- Align the dashboard frontend with the authoritative `DashboardOverview` backend DTO.
- Remove the dashboard's duplicate `/monitors` request and consume the backend aggregate in one request.
- Harden the frontend API error model for 409/429, structured error payloads, Retry-After, safe 5xx messages, and deterministic empty-body handling.
- Separate actual 401 session expiry from network/server failures in `RequireAuth`.
- Correct dashboard aggregation so disabled monitors are not counted as up/down.
- Add source-derived contract audit documentation and an executable contract-audit helper.

### Status
SOURCE CHANGES COMPLETE; LOCAL RUNTIME VERIFICATION REQUIRED.

### Verification
The authoring environment does not contain the Rust toolchain (`cargo` was unavailable), so the required workspace checks could not be executed here. Run locally before release:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cd apps/leptos-dashboard && trunk build --release`

### Architecture compliance
- No rewrite of backend architecture.
- Dashboard remains tenant-scoped through the existing authenticated use-case/repository chain.
- Existing LATERAL aggregation and database indexes are preserved.
- No unsafe automatic retries were introduced.
- Existing UI primitives remain in use.

PINGVERTEX_PHASE1_EOF

mkdir -p "$(dirname "apps/leptos-dashboard/HANDOVER.md")"
cat > "apps/leptos-dashboard/HANDOVER.md" <<'PINGVERTEX_PHASE1_EOF'
# HANDOVER — L5 Node.js Removal & Final Verification

## Worker
L5 — completed after verifying L1-L4 work.

## Scope
Remove Node.js frontend and fix URL prefix for public status API. Also fixed clippy warnings.

## Status
COMPLETE

## Files Changed
- `src/public_api.rs` — fixed `API_BASE` to `/api/v1/public`.
- `src/components/button.rs` — added `#[allow(dead_code)]`.
- `src/components/card.rs` — added `#[allow(dead_code)]`.

## Tests
Passed (see root HANDOVER for details).

## Notes
- Public status endpoints are now `/api/v1/public/status` and `/api/v1/public/status/{subdomain}`.

## Phase 1 — Backend ↔ Frontend Integration Hardening (2026-09-03)

### Status
SOURCE CHANGES COMPLETE; LOCAL RUNTIME VERIFICATION REQUIRED.

### Changes
- Dashboard DTO now mirrors backend `DashboardOverview` exactly.
- Dashboard uses one aggregated `/api/v1/dashboard/overview` request instead of overview + monitors.
- API client explicitly handles 409 Conflict and 429 Rate Limited and parses structured backend errors.
- 5xx details are sanitized before reaching UI; Retry-After is preserved when numeric.
- `RequireAuth` redirects only on a real 401 and keeps network/server failures retryable.
- Dashboard backend counts disabled monitors separately from up/down.

### Verification
Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cd apps/leptos-dashboard && trunk build --release` locally.

PINGVERTEX_PHASE1_EOF

chmod +x scripts/audit_frontend_backend_contracts.sh
./scripts/audit_frontend_backend_contracts.sh

echo
echo "=== REQUIRED VERIFICATION ==="
echo "cargo fmt --all"
echo "cargo clippy --workspace --all-targets -- -D warnings"
echo "cargo test --workspace"
echo "cd apps/leptos-dashboard && trunk build --release"
echo "cd ../.."
echo
echo "Phase 1 source changes installed. Runtime verification must pass locally before release."
