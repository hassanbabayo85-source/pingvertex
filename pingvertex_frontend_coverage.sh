#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-.}"
cd "$ROOT"

# PingVertex frontend coverage expansion.
# This script is intentionally explicit: every changed text file is written
# with a quoted heredoc so no shell interpolation can corrupt Rust/CSS content.

mkdir -p "$(dirname "apps/leptos-dashboard/Cargo.toml")"
cat > "apps/leptos-dashboard/Cargo.toml" <<'EOF_PINGVERTEX'
[package]
name = "leptos-dashboard"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
leptos = { version = "0.7", features = ["csr"] }
leptos_router = "0.7"
gloo-net = "0.6"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
wasm-bindgen = "0.2.93"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = ["RequestCredentials", "FormData", "Blob", "BlobPropertyBag", "HtmlInputElement", "File", "FileList"] }
js-sys = "0.3"
console_error_panic_hook = "0.1"
console_log = "1"
log = "0.4"
cfg-if = "1"
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/account.rs")"
cat > "apps/leptos-dashboard/src/api/account.rs" <<'EOF_PINGVERTEX'
//! Account-level read helpers for the Security page.
//!
//! NOTE for future Paths / the orchestrator: as of this pass, the backend
//! has no dedicated "account profile" endpoints (no `GET/PUT /account`).
//! Every account fact the frontend can currently read — email, username,
//! `email_verified`, role — comes from `GET /auth/me` (wrapped by
//! `auth::api::fetch_current_user`). This module exists as the place to
//! grow real account endpoints into later (per Path 7's file ownership),
//! and for now re-exports the session fetch under an account-flavored name
//! so `pages/security.rs` doesn't reach into `auth::api` directly for
//! profile data. Do not add fields here that the backend doesn't return —
//! see the confirmed backend gap below.
//!
pub use crate::auth::context::User as AccountProfile;

/// Fetches the current user's profile via the existing session endpoint.
/// See the module-level doc comment for why this is a thin re-export
/// rather than a dedicated `/account` call.
pub async fn fetch_account_profile() -> Result<AccountProfile, String> {
    crate::auth::api::fetch_current_user().await
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/advanced_monitoring.rs")"
cat > "apps/leptos-dashboard/src/api/advanced_monitoring.rs" <<'EOF_PINGVERTEX'
use crate::api::{client, monitoring};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MaintenanceWindow { pub id: String, pub monitor_id: String, pub starts_at: String, pub ends_at: String, pub reason: String, pub created_at: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EscalationPolicy { pub id: String, pub monitor_id: Option<String>, pub steps: Vec<EscalationStep>, pub created_at: String, pub updated_at: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EscalationStep { pub delay_minutes: u32, pub channel_id: String }

pub async fn list_maintenance(monitor_id: &str) -> Result<Vec<MaintenanceWindow>, client::ApiError> { client::get_json(&format!("/monitors/{monitor_id}/maintenance-windows")).await }
pub async fn create_maintenance(monitor_id: &str, req: &monitoring::MaintenanceWindowPayload) -> Result<MaintenanceWindow, client::ApiError> { client::post_json(&format!("/monitors/{monitor_id}/maintenance-windows"), req).await }
pub async fn delete_maintenance(id: &str) -> Result<(), client::ApiError> { client::delete_json_no_content(&format!("/maintenance-windows/{id}")).await }

#[derive(Debug, Clone, Serialize)]
pub struct EscalationPolicyPayload { pub monitor_id: Option<String>, pub steps: Vec<EscalationStepPayload> }
#[derive(Debug, Clone, Serialize)]
pub struct EscalationStepPayload { pub delay_minutes: u32, pub channel_id: String }
pub async fn list_escalations() -> Result<Vec<EscalationPolicy>, client::ApiError> { client::get_json("/escalation-policies").await }
pub async fn create_escalation(req: &EscalationPolicyPayload) -> Result<EscalationPolicy, client::ApiError> { client::post_json("/escalation-policies", req).await }
pub async fn update_escalation(id: &str, req: &EscalationPolicyPayload) -> Result<EscalationPolicy, client::ApiError> { client::put_json(&format!("/escalation-policies/{id}"), req).await }
pub async fn delete_escalation(id: &str) -> Result<(), client::ApiError> { client::delete_json_no_content(&format!("/escalation-policies/{id}")).await }
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/billing.rs")"
cat > "apps/leptos-dashboard/src/api/billing.rs" <<'EOF_PINGVERTEX'
use crate::api::client::{self, ApiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AddOn { pub id: String, pub addon_type: String, pub quantity: u64, pub tier: Option<String>, pub unit_price: String, pub total_price: String, pub currency: String, pub status: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BillingSummary { pub base_price: String, pub add_ons: Vec<AddOn>, pub add_ons_total: String, pub total: String, pub currency: String, pub pricing_version: u32 }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Quote { pub is_free: bool, pub monitors: u64, pub interval_seconds: u32, pub retention_days: u32, pub interval_label: String, pub retention_label: String, pub checks_per_period: u64, pub compute: CostLine, pub network: CostLine, pub storage: StorageCost, pub backup: CostLine, pub infrastructure_cost: String, pub markup_amount: String, pub monthly_price: String, pub currency: String, pub pricing_version: u32, pub generated_at: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CostLine { pub base: String, pub buffer: String, pub adjusted: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StorageCost { pub base: String, pub buffer: String, pub adjusted: String, pub storage_breakdown: Vec<StorageTierCost> }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StorageTierCost { pub tier: String, pub range_days: String, pub volume_gb: String, pub base_cost: String }
#[derive(Debug, Clone, Serialize)]
pub struct QuoteRequest { pub monitors: u64, pub interval_seconds: u32, pub retention_days: u32 }
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutRequest { pub plan_tier: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CheckoutResponse { pub checkout_session_id: String, pub checkout_url: String }
#[derive(Debug, Clone, Serialize)]
pub struct VerifyCheckoutRequest { pub checkout_session_id: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct VerifyCheckoutResponse { pub status: String }
#[derive(Debug, Clone, Serialize)]
pub struct AddOnRequest { pub addon_type: String, pub quantity: u64, pub country_code: Option<String>, pub tier: Option<String> }

pub async fn summary() -> Result<BillingSummary, ApiError> { client::get_json("/billing/summary").await }
pub async fn quote(req: &QuoteRequest) -> Result<Quote, ApiError> { client::post_json("/pricing/quote", req).await }
pub async fn checkout(plan_tier: &str) -> Result<CheckoutResponse, ApiError> { client::post_json("/billing/checkout", &CheckoutRequest { plan_tier: plan_tier.to_string() }).await }
pub async fn verify_checkout(id: &str) -> Result<VerifyCheckoutResponse, ApiError> { client::post_json("/billing/checkout/verify", &VerifyCheckoutRequest { checkout_session_id: id.to_string() }).await }
pub async fn add_on(req: &AddOnRequest) -> Result<AddOn, ApiError> { client::post_json("/billing/addons", req).await }
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/client.rs")"
cat > "apps/leptos-dashboard/src/api/client.rs" <<'EOF_PINGVERTEX'
use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use serde::Serialize;
use web_sys::{FormData, RequestCredentials};

const API_BASE: &str = "/api/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    Validation(String),
    NotFound,
    Network(String),
    Server(u16, String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Unauthorized => write!(f, "Unauthorized (401)"),
            ApiError::Forbidden => write!(f, "Forbidden (403)"),
            ApiError::Validation(msg) => write!(f, "Validation error: {msg}"),
            ApiError::NotFound => write!(f, "Not found (404)"),
            ApiError::Network(msg) => write!(f, "Network error: {msg}"),
            ApiError::Server(status, msg) => write!(f, "Server error ({status}): {msg}"),
        }
    }
}

impl From<gloo_net::Error> for ApiError {
    fn from(err: gloo_net::Error) -> Self {
        ApiError::Network(err.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::Validation(format!("Invalid JSON: {err}"))
    }
}

fn map_status(status: u16, body: String) -> ApiError {
    match status {
        401 => ApiError::Unauthorized,
        403 => ApiError::Forbidden,
        400 | 422 => ApiError::Validation(body),
        404 => ApiError::NotFound,
        500..=599 => ApiError::Server(status, body),
        _ => ApiError::Server(status, body),
    }
}

async fn handle_response<T: DeserializeOwned>(
    resp: gloo_net::http::Response,
) -> Result<T, ApiError> {
    let status = resp.status();
    if (200..300).contains(&status) {
        if status == 204 {
            return serde_json::from_str("null").map_err(ApiError::from);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        serde_json::from_str(&text).map_err(ApiError::from)
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
    }
}

pub async fn get_json<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::get(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    handle_response(resp).await
}

pub async fn post_json<T: DeserializeOwned, B: Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url)
        .header("Content-Type", "application/json")
        .credentials(RequestCredentials::Include)
        .json(body)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    handle_response(resp).await
}

pub async fn put_json<T: DeserializeOwned, B: Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::put(&url)
        .header("Content-Type", "application/json")
        .credentials(RequestCredentials::Include)
        .json(body)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    handle_response(resp).await
}

pub async fn post_json_no_content(path: &str) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    let status = resp.status();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
    }
}

pub async fn delete_json_no_content(path: &str) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::delete(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    let status = resp.status();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
    }
}


pub async fn get_json_query<T: DeserializeOwned>(
    path: &str,
    query: &[(&str, &str)],
) -> Result<T, ApiError> {
    let mut url = format!("{API_BASE}{path}");
    if !query.is_empty() {
        let encoded = query.iter().map(|(k, v)| {
            let key = js_sys::encode_uri_component(k).as_string().unwrap_or_default();
            let value = js_sys::encode_uri_component(v).as_string().unwrap_or_default();
            format!("{key}={value}")
        }).collect::<Vec<_>>().join("&");
        url.push('?');
        url.push_str(&encoded);
    }
    let resp = Request::get(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    handle_response(resp).await
}

pub async fn delete_json<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::delete(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    handle_response(resp).await
}

pub async fn put_json_no_content<B: Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::put(&url)
        .header("Content-Type", "application/json")
        .credentials(RequestCredentials::Include)
        .json(body)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    let status = resp.status();
    if (200..300).contains(&status) { Ok(()) } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
    }
}

pub async fn patch_json_no_content<B: Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::patch(&url)
        .header("Content-Type", "application/json")
        .credentials(RequestCredentials::Include)
        .json(body)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    let status = resp.status();
    if (200..300).contains(&status) { Ok(()) } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
    }
}

pub async fn post_multipart_file<T: DeserializeOwned>(
    path: &str,
    field_name: &str,
    file_name: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> Result<T, ApiError> {
    let form = FormData::new().map_err(|_| ApiError::Validation("could not create upload form".into()))?;
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&array);
    let mut options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &options)
        .map_err(|_| ApiError::Validation("could not create upload payload".into()))?;
    form.append_with_blob_and_filename(field_name, &blob, file_name)
        .map_err(|_| ApiError::Validation("could not attach upload".into()))?;
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url)
        .credentials(RequestCredentials::Include)
        .body(form)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    handle_response(resp).await
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/developer.rs")"
cat > "apps/leptos-dashboard/src/api/developer.rs" <<'EOF_PINGVERTEX'
use crate::api::client::{self, ApiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreatedApiKey { pub id: String, pub name: String, pub key: String, pub key_prefix: String, pub scopes: Vec<String>, pub created_at: String, pub expires_at: Option<String> }
#[derive(Debug, Clone, Serialize)]
pub struct CreateApiKeyRequest { pub name: String, pub scopes: Vec<String>, pub expires_at: Option<String> }

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiFlow { pub id: String, pub monitor_id: String, pub steps: Vec<ApiFlowStep>, pub created_at: String, pub updated_at: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiFlowStep { pub order: u16, pub name: String, pub method: String, pub url_template: String, pub headers: Vec<(String,String)>, pub body: Option<String>, pub extract: Vec<Extraction>, pub assertions: Vec<Assertion> }
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag="type", rename_all="snake_case")]
pub enum Extraction { StatusCode, Header { name: String }, JsonPath { path: String }, Body }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Assertion { pub id: String, pub response_part: String, pub operator: String, pub target: Option<String>, pub expected_value: Option<String> }
#[derive(Debug, Clone, Serialize)]
pub struct CreateApiFlowRequest { pub steps: Vec<ApiFlowStepRequest> }
#[derive(Debug, Clone, Serialize)]
pub struct ApiFlowStepRequest { pub order: u16, pub name: String, pub method: String, pub url_template: String, pub headers: Vec<(String,String)>, pub body: Option<String>, pub extract: Vec<ExtractionRequest>, pub assertions: Vec<AssertionRequest> }
#[derive(Debug, Clone, Serialize)]
#[serde(tag="type", rename_all="snake_case")]
pub enum ExtractionRequest { StatusCode, Header { name: String }, JsonPath { path: String }, Body }
#[derive(Debug, Clone, Serialize)]
pub struct AssertionRequest { pub response_part: String, pub operator: String, pub target: Option<String>, pub expected_value: Option<String> }

pub const API_KEY_SCOPES: &[&str] = &[
    "monitor:read", "monitor:write", "status_page:read", "status_page:write",
    "incident:read", "incident:write", "notification:read", "notification:write",
    "api_flow:read", "api_flow:write",
];

pub async fn list_keys() -> Result<Vec<ApiKey>, ApiError> { client::get_json("/api-keys").await }
pub async fn create_key(request: &CreateApiKeyRequest) -> Result<CreatedApiKey, ApiError> { client::post_json("/api-keys", request).await }
pub async fn delete_key(id: &str) -> Result<(), ApiError> { client::delete_json_no_content(&format!("/api-keys/{id}")).await }
pub async fn list_flows(monitor_id: &str) -> Result<Vec<ApiFlow>, ApiError> { client::get_json(&format!("/monitors/{monitor_id}/flows")).await }
pub async fn create_flow(monitor_id: &str, request: &CreateApiFlowRequest) -> Result<ApiFlow, ApiError> { client::post_json(&format!("/monitors/{monitor_id}/flows"), request).await }
pub async fn update_flow(monitor_id: &str, flow_id: &str, request: &CreateApiFlowRequest) -> Result<ApiFlow, ApiError> { client::put_json(&format!("/monitors/{monitor_id}/flows/{flow_id}"), request).await }
pub async fn delete_flow(monitor_id: &str, flow_id: &str) -> Result<(), ApiError> { client::delete_json_no_content(&format!("/monitors/{monitor_id}/flows/{flow_id}")).await }
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/mod.rs")"
cat > "apps/leptos-dashboard/src/api/mod.rs" <<'EOF_PINGVERTEX'
pub mod account;
pub mod client;
pub mod monitoring;
pub mod rum;
pub mod status_pages;
pub mod team;
pub mod developer;
pub mod billing;
pub mod support;
pub mod advanced_monitoring;
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/monitoring.rs")"
cat > "apps/leptos-dashboard/src/api/monitoring.rs" <<'EOF_PINGVERTEX'
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
pub struct Overview {
    pub total_monitors: u64,
    pub up_monitors: u64,
    pub down_monitors: u64,
    pub uptime_percentage: f64,
    pub average_response_time_ms: f64,
    pub active_incidents: u64,
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

pub async fn fetch_overview() -> Result<Overview, String> {
    client::get_json::<Overview>("/dashboard/overview")
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
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/rum.rs")"
cat > "apps/leptos-dashboard/src/api/rum.rs" <<'EOF_PINGVERTEX'
use js_sys::encode_uri_component;
use serde::{Deserialize, Serialize};

use crate::api::client::{self, ApiError};


fn build_url_with_query(path: &str, query: &[(&str, &str)]) -> String {
    if query.is_empty() {
        return path.to_string();
    }
    let mut parts = Vec::new();
    for (k, v) in query {
        let key = encode_uri_component(k).as_string().unwrap_or_default();
        let value = encode_uri_component(v).as_string().unwrap_or_default();
        parts.push(format!("{}={}", key, value));
    }
    format!("{}?{}", path, parts.join("&"))
}

// ---------- NEW DTOs (backend contract) ----------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RumSite {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub site_key: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CreateRumSitePayload {
    pub name: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendRumOverview {
    pub page_view_count: i64,
    pub session_count: i64,
    pub avg_load_time_ms: Option<f64>,
    pub p95_load_time_ms: Option<f64>,
    pub p75_lcp_ms: Option<f64>,
    pub p75_inp_ms: Option<f64>,
    pub p75_cls: Option<f64>,
    pub js_error_count: i64,
    pub resource_error_count: i64,
    pub error_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendPagePerformanceRow {
    pub path: String,
    pub view_count: i64,
    pub avg_load_time_ms: Option<f64>,
    pub p95_load_time_ms: Option<f64>,
    pub avg_lcp_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendTopJsErrorRow {
    pub message: String,
    pub occurrence_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendTopResourceErrorRow {
    pub resource_url: String,
    pub occurrence_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendRumErrorBreakdown {
    pub js_error_count: i64,
    pub resource_error_count: i64,
    pub page_view_count: i64,
    pub error_rate: Option<f64>,
    pub top_js_errors: Vec<BackendTopJsErrorRow>,
    pub top_resource_errors: Vec<BackendTopResourceErrorRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendBrowserBreakdownRow {
    pub browser_name: Option<String>,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendDeviceBreakdownRow {
    pub device_type: Option<String>,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendBrowserDeviceBreakdown {
    pub browsers: Vec<BackendBrowserBreakdownRow>,
    pub devices: Vec<BackendDeviceBreakdownRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendGeoBreakdownRow {
    pub country_code: Option<String>,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendPerformanceTrendPoint {
    pub bucket_start: String,
    pub page_view_count: i64,
    pub avg_load_time_ms: Option<f64>,
    pub avg_lcp_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendSessionStats {
    pub session_count: i64,
    pub avg_page_views_per_session: Option<f64>,
    pub avg_session_duration_secs: Option<f64>,
}

// ---------- New API functions (backend contract, typed) ----------

pub async fn list_rum_sites_typed() -> Result<Vec<RumSite>, ApiError> {
    let url = build_url_with_query("/dashboard/rum/sites", &[]);
    client::get_json::<Vec<RumSite>>(&url).await
}

pub async fn create_rum_site_typed(payload: &CreateRumSitePayload) -> Result<RumSite, ApiError> {
    client::post_json::<RumSite, _>("/dashboard/rum/sites", payload).await
}

pub async fn rum_overview_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<BackendRumOverview, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let path = format!("/dashboard/rum/sites/{}/overview", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<BackendRumOverview>(&url).await
}

pub async fn rum_slow_pages_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<BackendPagePerformanceRow>, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let limit_str;
    if let Some(l) = limit {
        limit_str = l.to_string();
        query.push(("limit", &limit_str));
    }
    let path = format!("/dashboard/rum/sites/{}/pages", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<Vec<BackendPagePerformanceRow>>(&url).await
}

pub async fn rum_error_breakdown_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    limit: Option<i64>,
) -> Result<BackendRumErrorBreakdown, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let limit_str;
    if let Some(l) = limit {
        limit_str = l.to_string();
        query.push(("limit", &limit_str));
    }
    let path = format!("/dashboard/rum/sites/{}/errors", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<BackendRumErrorBreakdown>(&url).await
}

pub async fn rum_browser_device_breakdown_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<BackendBrowserDeviceBreakdown, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let path = format!("/dashboard/rum/sites/{}/browsers-devices", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<BackendBrowserDeviceBreakdown>(&url).await
}

pub async fn rum_geo_breakdown_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<BackendGeoBreakdownRow>, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let limit_str;
    if let Some(l) = limit {
        limit_str = l.to_string();
        query.push(("limit", &limit_str));
    }
    let path = format!("/dashboard/rum/sites/{}/geo", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<Vec<BackendGeoBreakdownRow>>(&url).await
}

pub async fn rum_performance_trend_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Vec<BackendPerformanceTrendPoint>, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let path = format!("/dashboard/rum/sites/{}/trend", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<Vec<BackendPerformanceTrendPoint>>(&url).await
}

pub async fn rum_session_stats_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<BackendSessionStats, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let path = format!("/dashboard/rum/sites/{}/sessions", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<BackendSessionStats>(&url).await
}

EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/status_pages.rs")"
cat > "apps/leptos-dashboard/src/api/status_pages.rs" <<'EOF_PINGVERTEX'
use crate::api::{client, monitoring::Monitor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StatusPage {
    pub id: String,
    pub name: String,
    pub subdomain: String,
    pub custom_domain: Option<String>,
    pub custom_domain_verified: bool,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub monitor_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateStatusPageRequest {
    pub name: String,
    pub subdomain: String,
    pub monitor_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatusPageRequest {
    pub name: String,
    pub monitor_ids: Vec<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CustomDomainRequest {
    pub domain: String,
}

pub async fn list() -> Result<Vec<StatusPage>, client::ApiError> {
    client::get_json("/status-pages").await
}

pub async fn create(request: &CreateStatusPageRequest) -> Result<StatusPage, client::ApiError> {
    client::post_json("/status-pages", request).await
}

pub async fn update(id: &str, request: &UpdateStatusPageRequest) -> Result<StatusPage, client::ApiError> {
    client::put_json(&format!("/status-pages/{id}"), request).await
}

pub async fn delete(id: &str) -> Result<(), client::ApiError> {
    client::delete_json_no_content(&format!("/status-pages/{id}")).await
}

pub async fn request_custom_domain(id: &str, domain: &str) -> Result<StatusPage, client::ApiError> {
    client::post_json(
        &format!("/status-pages/{id}/custom-domain"),
        &CustomDomainRequest { domain: domain.to_string() },
    ).await
}

pub async fn remove_custom_domain(id: &str) -> Result<StatusPage, client::ApiError> {
    client::delete_json(&format!("/status-pages/{id}/custom-domain")).await
}

pub async fn upload_logo(id: &str, bytes: Vec<u8>, file_name: &str, mime: &str) -> Result<StatusPage, client::ApiError> {
    client::post_multipart_file(&format!("/status-pages/{id}/logo"), "logo", file_name, mime, bytes).await
}

pub async fn monitors() -> Result<Vec<Monitor>, client::ApiError> {
    client::get_json("/monitors").await
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/support.rs")"
cat > "apps/leptos-dashboard/src/api/support.rs" <<'EOF_PINGVERTEX'
use crate::api::client::{self, ApiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HelpCategory { pub id: String, pub name: String, pub slug: String, pub description: String, pub sort_order: i32 }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HelpArticle { pub id: String, pub category_id: String, pub title: String, pub slug: String, pub content: String, pub status: String, pub created_at: String, pub updated_at: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Ticket { pub id: String, pub ticket_number: String, pub subject: String, pub category: String, pub status: String, pub created_at: String, pub updated_at: String }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Message { pub id: String, pub ticket_id: String, pub author_id: String, pub message: String, pub created_at: String }
#[derive(Debug, Clone, Serialize)]
pub struct CreateTicket { pub subject: String, pub category: String }
#[derive(Debug, Clone, Serialize)]
pub struct Reply { pub message: String }
#[derive(Debug, Clone, Serialize)]
pub struct StatusRequest { pub status: String }

pub async fn categories() -> Result<Vec<HelpCategory>, ApiError> { client::get_json("/help/categories").await }
pub async fn articles(category: Option<&str>, q: Option<&str>) -> Result<Vec<HelpArticle>, ApiError> {
    let mut query = Vec::new(); if let Some(v)=category { query.push(("category", v)); } if let Some(v)=q { query.push(("q", v)); }
    client::get_json_query("/help/articles", &query).await
}
pub async fn article(slug: &str) -> Result<HelpArticle, ApiError> { client::get_json(&format!("/help/articles/{slug}")).await }
pub async fn search(q: &str) -> Result<Vec<HelpArticle>, ApiError> { client::get_json_query("/help/search", &[("q", q)]).await }
pub async fn tickets() -> Result<Vec<Ticket>, ApiError> { client::get_json("/support/tickets").await }
pub async fn ticket(id: &str) -> Result<(Ticket, Vec<Message>), ApiError> { client::get_json(&format!("/support/tickets/{id}")).await }
pub async fn create_ticket(req: &CreateTicket) -> Result<Ticket, ApiError> { client::post_json("/support/tickets", req).await }
pub async fn reply(id: &str, message: &str) -> Result<Message, ApiError> { client::post_json(&format!("/support/tickets/{id}/messages"), &Reply { message: message.to_string() }).await }
pub async fn set_status(id: &str, status: &str) -> Result<(), ApiError> { client::patch_json_no_content(&format!("/support/tickets/{id}/status"), &StatusRequest { status: status.to_string() }).await }
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/api/team.rs")"
cat > "apps/leptos-dashboard/src/api/team.rs" <<'EOF_PINGVERTEX'
use crate::api::client::{self, ApiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Member {
    pub user_id: String,
    pub role: String,
    pub invited_at: String,
    pub joined_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Invitation {
    pub id: String,
    pub email: String,
    pub role: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreatedInvitation {
    pub id: String,
    pub email: String,
    pub role: String,
    pub expires_at: String,
    pub created_at: String,
    pub invitation_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddMemberRequest { pub user_id: String, pub role: String }
#[derive(Debug, Clone, Serialize)]
pub struct RoleRequest { pub role: String }
#[derive(Debug, Clone, Serialize)]
pub struct CreateInvitationRequest { pub email: String, pub role: String }
#[derive(Debug, Clone, Serialize)]
pub struct AcceptInvitationRequest { pub token: String }

pub async fn list_members() -> Result<Vec<Member>, ApiError> { client::get_json("/members").await }
pub async fn add_member(request: &AddMemberRequest) -> Result<Member, ApiError> { client::post_json("/members", request).await }
pub async fn change_role(user_id: &str, role: &str) -> Result<(), ApiError> {
    client::put_json_no_content(&format!("/members/{user_id}/role"), &RoleRequest { role: role.to_string() }).await
}
pub async fn remove_member(user_id: &str) -> Result<(), ApiError> { client::delete_json_no_content(&format!("/members/{user_id}")).await }
pub async fn list_invitations() -> Result<Vec<Invitation>, ApiError> { client::get_json("/invitations/pending").await }
pub async fn create_invitation(request: &CreateInvitationRequest) -> Result<CreatedInvitation, ApiError> { client::post_json("/invitations", request).await }
pub async fn revoke_invitation(id: &str) -> Result<(), ApiError> { client::delete_json_no_content(&format!("/invitations/{id}")).await }
pub async fn accept_invitation(token: &str) -> Result<Member, ApiError> { client::post_json("/invitations/accept", &AcceptInvitationRequest { token: token.to_string() }).await }
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/auth/context.rs")"
cat > "apps/leptos-dashboard/src/auth/context.rs" <<'EOF_PINGVERTEX'
#![allow(dead_code)]
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Role {
    Member,
    Admin,
    Owner,
    Auditor,
    Viewer,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    pub email_verified: bool,
    pub tenant_id: String,
    pub role: Role,
    pub totp_enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AuthState {
    Unknown,
    Authenticated(User),
    Unauthenticated,
    TwoFactorRequired { pending_token: String },
}

#[derive(Clone, Copy)]
pub struct AuthContext {
    pub state: RwSignal<AuthState>,
}

pub fn provide_auth_context() {
    let state = RwSignal::new(AuthState::Unknown);
    provide_context(AuthContext { state });
}

pub fn use_auth() -> AuthContext {
    use_context::<AuthContext>().expect("AuthContext not provided")
}

impl AuthContext {
    pub fn set_state(&self, new_state: AuthState) {
        self.state.set(new_state);
    }
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/components/navigation.rs")"
cat > "apps/leptos-dashboard/src/components/navigation.rs" <<'EOF_PINGVERTEX'
//! Navigation model for the authenticated app shell.
//!
//! This module is intentionally data-only: it describes which product areas
//! exist, which route each one points at, and whether that route is
//! registered yet. `components::layout::Layout` renders this data. Keeping
//! the data separate from the render code means Path 10 (or any future
//! integration pass) can flip an item from `NavStatus::ComingSoon` to
//! `NavStatus::Available` in one place, in `routes/mod.rs`-adjacent order,
//! without touching rendering logic.
//!
//! IMPORTANT: adding an item here does NOT register a route. A route must
//! exist in `routes/mod.rs` (owned by Path 10) before its `NavStatus` is
//! flipped to `Available`, otherwise the link would 404 or fall through to
//! `NotFoundPage`.

/// Whether a nav target is safe to click today.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NavStatus {
    /// The route is registered in `routes/mod.rs` and the page exists.
    Available,
    /// The feature is planned (owned by another Path) but not routable yet.
    /// Rendered as a disabled, non-navigating list item so it never becomes
    /// a dead link.
    ComingSoon,
}

#[derive(Clone, Copy)]
pub struct NavItem {
    pub label: &'static str,
    pub href: &'static str,
    pub status: NavStatus,
}

#[derive(Clone, Copy)]
pub struct NavGroup {
    pub label: &'static str,
    pub items: &'static [NavItem],
}

// Single source of truth for the sidebar. Update the `status` field (only)
// once the corresponding route lands in `routes/mod.rs` — do not add new
// hrefs speculatively; every href here must correspond to a page a Path has
// actually delivered or is actively delivering.
pub const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
        label: "OVERVIEW",
        items: &[NavItem {
            label: "Dashboard",
            href: "/",
            status: NavStatus::Available,
        }],
    },
    NavGroup {
        label: "MONITORING",
        items: &[
            NavItem {
                label: "Monitors",
                href: "/monitors",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Incidents",
                href: "/monitors",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Maintenance windows",
                href: "/maintenance",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Escalation policies",
                href: "/escalation-policies",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "USER EXPERIENCE",
        items: &[
            NavItem {
                label: "Real User Monitoring",
                href: "/rum",
                status: NavStatus::Available,
            },
            NavItem {
                label: "RUM sites",
                href: "/rum/sites",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "PUBLIC",
        items: &[NavItem {
            label: "Status pages",
            href: "/status-pages",
            status: NavStatus::Available,
        }],
    },
    NavGroup {
        label: "TEAM",
        items: &[
            NavItem {
                label: "Members",
                href: "/team",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Invitations",
                href: "/team/invitations",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "DEVELOPER",
        items: &[
            NavItem {
                label: "API keys",
                href: "/developer/api-keys",
                status: NavStatus::Available,
            },
            NavItem {
                label: "API flows",
                href: "/developer/api-flows",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "BILLING",
        items: &[
            NavItem {
                label: "Overview",
                href: "/billing",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Plan & usage",
                href: "/billing/plan",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Add-ons",
                href: "/billing/add-ons",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "SUPPORT",
        items: &[
            NavItem {
                label: "Help center",
                href: "/help",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Support tickets",
                href: "/support",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "SETTINGS",
        items: &[
            NavItem {
                label: "Account",
                href: "/settings/account",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Security",
                href: "/settings/security",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Notifications",
                href: "/settings/notifications",
                status: NavStatus::Available,
            },
        ],
    },
];
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/account.rs")"
cat > "apps/leptos-dashboard/src/pages/account.rs" <<'EOF_PINGVERTEX'
use crate::api::account;
use leptos::prelude::*;use leptos::task::spawn_local;
#[component]
pub fn AccountPage()->impl IntoView{
 let profile=RwSignal::new(None::<account::AccountProfile>);let error=RwSignal::new(None::<String>);
 Effect::new(move |_|spawn_local(async move{match account::fetch_account_profile().await{Ok(v)=>profile.set(Some(v)),Err(e)=>error.set(Some(e))}}));
 view!{<div class="feature-page"><header class="page-header"><h1>"Account"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||profile.get().map(|p|view!{<section class="card"><dl class="details"><dt>"Username"</dt><dd>{p.username}</dd><dt>"Email"</dt><dd>{p.email}</dd><dt>"Email verified"</dt><dd>{if p.email_verified{"Yes"}else{"No"}}</dd><dt>"Role"</dt><dd>{format!("{:?}",p.role)}</dd><dt>"Tenant"</dt><dd><code>{p.tenant_id}</code></dd></dl></section>})}<p class="muted">"Profile editing is intentionally unavailable until the backend exposes a dedicated account-update contract."</p></div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/add_ons.rs")"
cat > "apps/leptos-dashboard/src/pages/add_ons.rs" <<'EOF_PINGVERTEX'
use crate::api::billing;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;use leptos::task::spawn_local;
#[component]
pub fn AddOnsPage()->impl IntoView{
 let auth=use_auth();let kind=RwSignal::new("enterprise".into());let qty=RwSignal::new("1".into());let country=RwSignal::new(String::new());let result=RwSignal::new(None::<billing::AddOn>);let error=RwSignal::new(None::<String>);
 let can=move||match auth.state.get(){AuthState::Authenticated(u)=>matches!(u.role,Role::Owner),_=>false};
 let add=move|e:leptos::ev::SubmitEvent|{e.prevent_default();let req=billing::AddOnRequest{addon_type:kind.get(),quantity:qty.get().parse().unwrap_or(0),country_code:if country.get().is_empty(){None}else{Some(country.get())},tier:None};spawn_local(async move{match billing::add_on(&req).await{Ok(v)=>result.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"Add-ons"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||result.get().map(|r|view!{<div class="success">{format!("Created {} × {} for {} {}",r.quantity,r.addon_type,r.total_price,r.currency)}</div>})}{move||if can(){view!{<section class="card"><form on:submit=add class="stack-form"><label>"Type"<select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))><option value="enterprise">"Enterprise SLA"</option><option value="sms">"SMS credits"</option></select></label><label>"Quantity"<input type="number" min="1" prop:value=qty on:input=move|e|qty.set(event_target_value(&e))/></label>{move||if kind.get()=="sms"{view!{<label>"Country code"<input placeholder="NG" prop:value=country on:input=move|e|country.set(event_target_value(&e)) required/></label>}.into_any()}else{().into_any()}}<button class="btn btn-primary">"Create add-on"</button></form></section>}.into_any()}else{view!{<p class="muted">"Only the tenant owner can manage add-ons."</p>}.into_any()}}</div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/api_flows.rs")"
cat > "apps/leptos-dashboard/src/pages/api_flows.rs" <<'EOF_PINGVERTEX'
use crate::api::{developer, monitoring};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Clone)]
struct StepDraft { name:String, method:String, url:String, headers:Vec<(String,String)>, body:Option<String>, extract:Vec<developer::ExtractionRequest>, assertions:Vec<developer::AssertionRequest> }
#[component]
pub fn ApiFlowsPage()->impl IntoView{
 let monitors=RwSignal::new(Vec::<monitoring::Monitor>::new());let selected=RwSignal::new(None::<String>);let flows=RwSignal::new(Vec::<developer::ApiFlow>::new());let steps=RwSignal::new(Vec::<StepDraft>::new());let step_name=RwSignal::new(String::new());let step_url=RwSignal::new(String::new());let step_method=RwSignal::new("GET".into());let editing=RwSignal::new(None::<String>);let error=RwSignal::new(None::<String>);let notice=RwSignal::new(None::<String>);
 Effect::new(move |_|spawn_local(async move{match monitoring::fetch_monitors().await{Ok(v)=>monitors.set(v),Err(e)=>error.set(Some(e))}}));
 Effect::new(move |_|{if let Some(id)=selected.get(){let id=id.clone();spawn_local(async move{match developer::list_flows(&id).await{Ok(v)=>flows.set(v),Err(e)=>error.set(Some(e.to_string()))}})}});
 let add=move|ev:leptos::ev::SubmitEvent|{ev.prevent_default();let n=step_name.get();let u=step_url.get();if n.trim().is_empty()||u.trim().is_empty(){error.set(Some("Step name and URL are required.".into()));return}steps.update(|v|v.push(StepDraft{name:n,method:step_method.get(),url:u,headers:Vec::new(),body:None,extract:Vec::new(),assertions:Vec::new()}));step_name.set(String::new());step_url.set(String::new());};
 let save=move|ev:leptos::ev::SubmitEvent|{ev.prevent_default();let Some(mid)=selected.get()else{return};let payload=developer::CreateApiFlowRequest{steps:steps.get().iter().enumerate().map(|(i,s)|developer::ApiFlowStepRequest{order:(i+1)as u16,name:s.name.clone(),method:s.method.clone(),url_template:s.url.clone(),headers:s.headers.clone(),body:s.body.clone(),extract:s.extract.clone(),assertions:s.assertions.clone()}).collect()};let mid2=mid.clone();spawn_local(async move{match match editing.get_untracked(){Some(flow_id)=>developer::update_flow(&mid2,&flow_id,&payload).await,None=>developer::create_flow(&mid2,&payload).await}{Ok(_)=>{editing.set(None);notice.set(Some("API flow saved.".into()));match developer::list_flows(&mid2).await{Ok(v)=>flows.set(v),Err(e)=>error.set(Some(e.to_string()))}},Err(e)=>error.set(Some(e.to_string()))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"API flows"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><label>"Monitor"<select on:change=move|e|{let v=event_target_value(&e);selected.set((!v.is_empty()).then_some(v));steps.set(Vec::new())}><option value="">"Select a monitor"</option>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<option value={id}>{m.name}</option>}}).collect_view()}</select></label></section>{move||selected.get().map(|_|view!{<section class="card"><h2>"Build flow"</h2><form on:submit=add class="inline-form"><input placeholder="Step name" prop:value=step_name on:input=move|e|step_name.set(event_target_value(&e)) required/><select prop:value=step_method on:change=move|e|step_method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select><input placeholder="https://example.com/api" prop:value=step_url on:input=move|e|step_url.set(event_target_value(&e)) required/><button class="btn">"Add step"</button></form><ol>{move||steps.get().into_iter().enumerate().map(|(i,s)|view!{<li class="list-row"><span>{format!("{}. {} — {} {}",i+1,s.name,s.method,s.url)}</span><button class="btn btn-danger" type="button" on:click=move |_|steps.update(|v|{if i<v.len(){v.remove(i)}})>"Remove"</button></li>}).collect_view()}</ol><button class="btn btn-primary" on:click=save disabled=move||steps.get().is_empty()>{move||if editing.get().is_some(){"Save flow"}else{"Create flow"}}</button></section>})}<section class="card"><h2>"Existing flows"</h2>{move||flows.get().into_iter().map(|f|{let id=f.id.clone();let loaded=f.steps.clone();view!{<article class="list-row"><div><strong>{format!("Flow {}",f.id)}</strong><div class="muted">{format!("{} step(s)",f.steps.len())}</div></div><button class="btn" on:click=move |_|{editing.set(Some(id.clone()));steps.set(loaded.iter().map(|s|StepDraft{name:s.name.clone(),method:s.method.clone(),url:s.url_template.clone(),headers:s.headers.clone(),body:s.body.clone(),extract:s.extract.iter().map(|x|match x{developer::Extraction::StatusCode=>developer::ExtractionRequest::StatusCode,developer::Extraction::Header{name}=>developer::ExtractionRequest::Header{name:name.clone()},developer::Extraction::JsonPath{path}=>developer::ExtractionRequest::JsonPath{path:path.clone()},developer::Extraction::Body=>developer::ExtractionRequest::Body}).collect(),assertions:s.assertions.iter().map(|x|developer::AssertionRequest{response_part:x.response_part.clone(),operator:x.operator.clone(),target:x.target.clone(),expected_value:x.expected_value.clone()}).collect()}).collect());}>"Edit"</button></article>}}).collect_view()}</section><p class="muted">"Flow URLs, headers and assertions are validated by the backend. This UI intentionally sends only fields supported by the current contract."</p></div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/api_keys.rs")"
cat > "apps/leptos-dashboard/src/pages/api_keys.rs" <<'EOF_PINGVERTEX'
use crate::api::developer;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_manage(role:&Role)->bool{matches!(role,Role::Admin|Role::Owner)}
#[component]
pub fn ApiKeysPage()->impl IntoView{
 let auth=use_auth();let keys=RwSignal::new(Vec::<developer::ApiKey>::new());let name=RwSignal::new(String::new());let scopes=RwSignal::new(Vec::<String>::new());let error=RwSignal::new(None::<String>);let secret=RwSignal::new(None::<String>);let busy=RwSignal::new(false);
 let can=move||match auth.state.get(){AuthState::Authenticated(u)=>can_manage(&u.role),_=>false};
 let reload=move||spawn_local(async move{match developer::list_keys().await{Ok(v)=>keys.set(v),Err(e)=>error.set(Some(e.to_string()))}});reload();
 let create=move|ev:leptos::ev::SubmitEvent|{ev.prevent_default();let n=name.get();let s=scopes.get();busy.set(true);spawn_local(async move{match developer::create_key(&developer::CreateApiKeyRequest{name:n,scopes:s,expires_at:None}).await{Ok(k)=>{secret.set(Some(k.key));reload();name.set(String::new());scopes.set(Vec::new());},Err(e)=>error.set(Some(e.to_string()))}busy.set(false);});};
 view!{<div class="feature-page"><header class="page-header"><h1>"API keys"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||secret.get().map(|s|view!{<div class="secret-box"><strong>"API key — copy it now"</strong><code>{s}</code><span class="muted">"It will not be shown again."</span></div>})}<section class="card"><h2>"Keys"</h2>{move||keys.get().into_iter().map(|k|{let id=k.id.clone();view!{<div class="list-row"><div><strong>{k.name}</strong><div class="muted"><code>{k.key_prefix}</code> · {k.scopes.join(", ")}</div></div>{move||if can(){view!{<button class="btn btn-danger" on:click=move |_|{let id=id.clone();spawn_local(async move{match developer::delete_key(&id).await{Ok(())=>keys.update(|v|v.retain(|x|x.id!=id)),Err(e)=>error.set(Some(e.to_string()))}})}>"Revoke"</button>}.into_any()}else{().into_any()}}</div>}}).collect_view()}</section>{move||if can(){view!{<section class="card"><h2>"Create key"</h2><form on:submit=create class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><fieldset><legend>"Scopes"</legend>{developer::API_KEY_SCOPES.iter().map(|scope|{let s=(*scope).to_string();view!{<label class="check-row"><input type="checkbox" on:change=move|e|{let checked=event_target::<web_sys::HtmlInputElement>(&e).checked();scopes.update(|v|if checked{if !v.contains(&s){v.push(s.clone())}}else{v.retain(|x|x!=&s)});}/>{*scope}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get()>"Create key"</button></form></section>}.into_any()}else{().into_any()}}<p class="muted">"The server enforces scope permissions. Secrets are never stored in browser storage."</p></div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/billing.rs")"
cat > "apps/leptos-dashboard/src/pages/billing.rs" <<'EOF_PINGVERTEX'
use crate::api::billing;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_bill(role:&Role)->bool{matches!(role,Role::Owner)}
#[component]
pub fn BillingPage()->impl IntoView{
 let auth=use_auth();let summary=RwSignal::new(None::<billing::BillingSummary>);let error=RwSignal::new(None::<String>);let notice=RwSignal::new(None::<String>);let plan=RwSignal::new("tier_1".into());let session=RwSignal::new(String::new());let checkout_status=RwSignal::new(None::<String>);let quote=RwSignal::new(None::<billing::Quote>);let monitors=RwSignal::new("1".into());let interval=RwSignal::new("60".into());let retention=RwSignal::new("30".into());
 let can=move||match auth.state.get(){AuthState::Authenticated(u)=>can_bill(&u.role),_=>false};
 Effect::new(move |_|spawn_local(async move{match billing::summary().await{Ok(v)=>summary.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}}));
 let get_quote=move|ev:leptos::ev::SubmitEvent|{ev.prevent_default();let req=billing::QuoteRequest{monitors:monitors.get().parse().unwrap_or(1),interval_seconds:interval.get().parse().unwrap_or(60),retention_days:retention.get().parse().unwrap_or(30)};spawn_local(async move{match billing::quote(&req).await{Ok(v)=>quote.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}});};
 let checkout=move||{let p=plan.get();spawn_local(async move{match billing::checkout(&p).await{Ok(v)=>{notice.set(Some("Checkout created. Continue in the payment provider.".into()));if let Some(w)=web_sys::window(){let _=w.location().set_href(&v.checkout_url);}},Err(e)=>error.set(Some(e.to_string()))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"Billing"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><h2>"Current billing"</h2>{move||summary.get().map(|s|view!{<div class="overview-cards"><div class="card"><span class="card-label">"Base"</span><span class="card-value">{format!("{} {}",s.base_price,s.currency)}</span></div><div class="card"><span class="card-label">"Add-ons"</span><span class="card-value">{format!("{} {}",s.add_ons_total,s.currency)}</span></div><div class="card"><span class="card-label">"Total"</span><span class="card-value">{format!("{} {}",s.total,s.currency)}</span></div></div>})}</section><section class="card"><h2>"Pricing quote"</h2><p class="muted">"The server is the only pricing authority; these values are sent to the quote endpoint and never calculated in the browser."</p><form on:submit=get_quote class="inline-form"><label>"Monitors"<input type="number" min="1" prop:value=monitors on:input=move|e|monitors.set(event_target_value(&e))/></label><label>"Interval (s)"<input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e))/></label><label>"Retention (days)"<input type="number" min="1" prop:value=retention on:input=move|e|retention.set(event_target_value(&e))/></label><button class="btn">"Get quote"</button></form>{move||quote.get().map(|q|view!{<div class="quote-box"><strong>{format!("{} {} / month",q.monthly_price,q.currency)}</strong><span>{q.interval_label}</span><span>{q.retention_label}</span></div>})}</section><section class="card"><h2>"Verify checkout"</h2><p class="muted">"After the provider completes payment, enter the checkout session ID returned by the server. Payment state is verified server-side."</p><form on:submit=move|e|{e.prevent_default();let id=session.get();spawn_local(async move{match billing::verify_checkout(&id).await{Ok(v)=>checkout_status.set(Some(v.status)),Err(e)=>error.set(Some(e.to_string()))}})} class="inline-form"><input placeholder="Checkout session ID" prop:value=session on:input=move|e|session.set(event_target_value(&e)) required/><button class="btn">"Verify"</button></form>{move||checkout_status.get().map(|v|view!{<p><strong>{v}</strong></p>})}</section>{move||if can(){view!{<section class="card"><h2>"Change plan"</h2><div class="inline-form"><select prop:value=plan on:change=move|e|plan.set(event_target_value(&e))><option value="free">"Free"</option><option value="tier_1">"Tier 1"</option><option value="tier_5">"Tier 5"</option><option value="tier_10">"Tier 10"</option><option value="tier_20">"Tier 20"</option></select><button class="btn btn-primary" on:click=move |_|checkout()>"Continue to checkout"</button></div></section>}.into_any()}else{view!{<p class="muted">"Only the tenant owner can change the subscription."</p>}.into_any()}}</div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/escalation.rs")"
cat > "apps/leptos-dashboard/src/pages/escalation.rs" <<'EOF_PINGVERTEX'
use crate::api::{advanced_monitoring as api, monitoring};
use leptos::prelude::*;use leptos::task::spawn_local;
#[component]
pub fn EscalationPage()->impl IntoView{
 let monitors=RwSignal::new(Vec::<monitoring::Monitor>::new());let channels=RwSignal::new(Vec::<monitoring::NotificationChannel>::new());let policies=RwSignal::new(Vec::<api::EscalationPolicy>::new());let selected_monitor=RwSignal::new(String::new());let selected_channel=RwSignal::new(String::new());let delay=RwSignal::new("5".into());let steps=RwSignal::new(Vec::<api::EscalationStepPayload>::new());let editing=RwSignal::new(None::<String>);let error=RwSignal::new(None::<String>);let notice=RwSignal::new(None::<String>);
 Effect::new(move |_|spawn_local(async move{match monitoring::fetch_monitors().await{Ok(v)=>monitors.set(v),Err(e)=>error.set(Some(e))}}));
 Effect::new(move |_|spawn_local(async move{match api::list_escalations().await{Ok(v)=>policies.set(v),Err(e)=>error.set(Some(e.to_string()))}}));
 Effect::new(move |_|{let id=selected_monitor.get();if id.is_empty(){channels.set(Vec::new());return}spawn_local(async move{match monitoring::list_notification_channels(&id).await{Ok(v)=>channels.set(v),Err(e)=>error.set(Some(e))}})});
 let add_step=move|e:leptos::ev::SubmitEvent|{e.prevent_default();let c=selected_channel.get();if c.is_empty(){error.set(Some("Select a notification channel.".into()));return}steps.update(|v|v.push(api::EscalationStepPayload{delay_minutes:delay.get().parse().unwrap_or(0),channel_id:c}));};
 let save=move|e:leptos::ev::SubmitEvent|{e.prevent_default();let req=api::EscalationPolicyPayload{monitor_id:if selected_monitor.get().is_empty(){None}else{Some(selected_monitor.get())},steps:steps.get()};let current=editing.get();spawn_local(async move{let r=match current{Some(id)=>api::update_escalation(&id,&req).await, None=>api::create_escalation(&req).await};match r{Ok(v)=>{policies.update(|p|{if let Some(i)=p.iter().position(|x|x.id==v.id){p[i]=v}else{p.insert(0,v)}});notice.set(Some("Escalation policy saved.".into()));},Err(e)=>error.set(Some(e.to_string()))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"Escalation policies"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<section class="card"><h2>"Create / edit policy"</h2><form on:submit=save class="stack-form"><label>"Monitor (optional)"<select prop:value=selected_monitor on:change=move|e|{selected_monitor.set(event_target_value(&e));steps.set(Vec::new())}><option value="">"Global"</option>{move||monitors.get().into_iter().map(|m|view!{<option value={m.id}>{m.name}</option>}).collect_view()}</select></label><div class="inline-form"><select prop:value=selected_channel on:change=move|e|selected_channel.set(event_target_value(&e))><option value="">"Channel"</option>{move||channels.get().into_iter().map(|c|view!{<option value={c.id}>{format!("{} — {}",c.kind,c.target)}</option>}).collect_view()}</select><input type="number" min="0" prop:value=delay on:input=move|e|delay.set(event_target_value(&e))/><button class="btn" on:click=add_step>"Add step"</button></div><ol>{move||steps.get().into_iter().map(|s|view!{<li>{format!("{} min → {}",s.delay_minutes,s.channel_id)}</li>}).collect_view()}</ol><button class="btn btn-primary" disabled=move||steps.get().is_empty()>"Save policy"</button></form></section><section class="card"><h2>"Existing policies"</h2>{move||policies.get().into_iter().map(|p|{let id=p.id.clone();view!{<div class="list-row"><div><strong>{p.monitor_id.clone().unwrap_or_else(||"Global".into())}</strong><div class="muted">{format!("{} step(s)",p.steps.len())}</div></div><button class="btn" on:click=move |_|{editing.set(Some(id.clone()));selected_monitor.set(p.monitor_id.clone().unwrap_or_default());steps.set(p.steps.clone());}>"Edit"</button></div>}}).collect_view()}</section></div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/help_center.rs")"
cat > "apps/leptos-dashboard/src/pages/help_center.rs" <<'EOF_PINGVERTEX'
use crate::api::support;
use leptos::prelude::*;use leptos::task::spawn_local;
#[component]
pub fn HelpCenterPage()->impl IntoView{
 let categories=RwSignal::new(Vec::<support::HelpCategory>::new());let articles=RwSignal::new(Vec::<support::HelpArticle>::new());let selected=RwSignal::new(None::<support::HelpArticle>);let q=RwSignal::new(String::new());let error=RwSignal::new(None::<String>);
 Effect::new(move |_|spawn_local(async move{match support::categories().await{Ok(v)=>categories.set(v),Err(e)=>error.set(Some(e.to_string()))}}));
 let search=move|e:leptos::ev::SubmitEvent|{e.prevent_default();let query=q.get();spawn_local(async move{match support::search(&query).await{Ok(v)=>articles.set(v),Err(e)=>error.set(Some(e.to_string()))}});};
 let load_cat=move|slug:String|{spawn_local(async move{match support::articles(Some(&slug),None).await{Ok(v)=>articles.set(v),Err(e)=>error.set(Some(e.to_string()))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"Help center"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<form on:submit=search class="inline-form"><input placeholder="Search help" prop:value=q on:input=move|e|q.set(event_target_value(&e))/><button class="btn">"Search"</button></form><section class="feature-grid"><div class="card"><h2>"Categories"</h2>{move||categories.get().into_iter().map(|c|{let slug=c.slug.clone();view!{<button class="link-button" on:click=move |_|load_cat(slug.clone())>{c.name}</button>}}).collect_view()}</div><div class="card"><h2>"Articles"</h2>{move||articles.get().into_iter().map(|a|{let slug=a.slug.clone();view!{<button class="list-row link-button" on:click=move |_|{let slug=slug.clone();spawn_local(async move{match support::article(&slug).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}})}>{a.title}</button>}}).collect_view()}</div></section>{move||selected.get().map(|a|view!{<article class="card article"><h2>{a.title}</h2><div class="article-content">{a.content}</div></article>})}</div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/invitations.rs")"
cat > "apps/leptos-dashboard/src/pages/invitations.rs" <<'EOF_PINGVERTEX'
use crate::api::team;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_manage(role: &Role) -> bool { matches!(role, Role::Admin | Role::Owner) }

#[component]
pub fn InvitationsPage() -> impl IntoView {
    let auth = use_auth();
    let invitations = RwSignal::new(Vec::<team::Invitation>::new());
    let error = RwSignal::new(None::<String>);
    let token = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let role = RwSignal::new("Member".into());
    let notice = RwSignal::new(None::<String>);
    let created_token = RwSignal::new(None::<String>);
    let can = move || match auth.state.get() { AuthState::Authenticated(u) => can_manage(&u.role), _ => false };
    let reload = move || spawn_local(async move {
        match team::list_invitations().await { Ok(v) => invitations.set(v), Err(e) => error.set(Some(e.to_string())) }
    });
    reload();
    let create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let req = team::CreateInvitationRequest { email: email.get(), role: role.get() };
        spawn_local(async move {
            match team::create_invitation(&req).await {
                Ok(v) => { created_token.set(Some(v.invitation_token)); notice.set(Some("Invitation created. Save the token now; it is returned only once.".into())); reload(); }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    let accept = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let t = token.get();
        spawn_local(async move {
            match team::accept_invitation(&t).await { Ok(_) => notice.set(Some("Invitation accepted.".into())), Err(e) => error.set(Some(e.to_string())) }
        });
    };
    view! {
        <div class="feature-page">
            <header class="page-header"><h1>"Invitations"</h1></header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || created_token.get().map(|t| view! { <div class="secret-box"><strong>"Invitation token — save it now"</strong><code>{t}</code></div> })}
            <div class="feature-grid">
                {move || if can() { view! { <section class="card"><h2>"Create invitation"</h2><form on:submit=create class="stack-form"><label>"Email"<input type="email" prop:value=email on:input=move |e| email.set(event_target_value(&e)) required/></label><label>"Role"<select prop:value=role on:change=move |e| role.set(event_target_value(&e))><option value="Member">"Member"</option><option value="Admin">"Admin"</option><option value="Auditor">"Auditor"</option><option value="Viewer">"Viewer"</option></select></label><button class="btn btn-primary">"Create"</button></form></section> }.into_any() } else { ().into_any() }}
                <section class="card"><h2>"Pending"</h2>{move || invitations.get().into_iter().map(|i| { let id=i.id.clone(); view! { <div class="list-row"><div><strong>{i.email}</strong><div class="muted">{i.role} · expires {i.expires_at}</div></div>{move || if can(){ let revoke_id=id.clone(); view!{<button class="btn btn-danger" on:click=move |_|{let revoke_id=revoke_id.clone();spawn_local(async move{match team::revoke_invitation(&revoke_id).await{Ok(())=>invitations.update(|v|v.retain(|x|x.id!=revoke_id)),Err(e)=>error.set(Some(e.to_string()))}})}>"Revoke"</button>}.into_any()}else{().into_any()}}</div> } }).collect_view()}</section>
            </div>
            <section class="card"><h2>"Accept invitation"</h2><form on:submit=accept class="inline-form"><input placeholder="Invitation token" prop:value=token on:input=move |e| token.set(event_target_value(&e)) required/><button class="btn">"Accept"</button></form></section>
        </div>
    }
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/maintenance.rs")"
cat > "apps/leptos-dashboard/src/pages/maintenance.rs" <<'EOF_PINGVERTEX'
use crate::api::{advanced_monitoring as api, monitoring};
use leptos::prelude::*;use leptos::task::spawn_local;
fn iso(value:String)->String{js_sys::Date::new(&wasm_bindgen::JsValue::from_str(&value)).to_iso_string().as_string().unwrap_or(value)}
#[component]
pub fn MaintenancePage()->impl IntoView{
 let monitors=RwSignal::new(Vec::<monitoring::Monitor>::new());let selected=RwSignal::new(None::<String>);let windows=RwSignal::new(Vec::<api::MaintenanceWindow>::new());let start=RwSignal::new(String::new());let end=RwSignal::new(String::new());let reason=RwSignal::new(String::new());let error=RwSignal::new(None::<String>);let notice=RwSignal::new(None::<String>);
 Effect::new(move |_|spawn_local(async move{match monitoring::fetch_monitors().await{Ok(v)=>monitors.set(v),Err(e)=>error.set(Some(e))}}));
 Effect::new(move |_|{if let Some(id)=selected.get(){let id=id.clone();spawn_local(async move{match api::list_maintenance(&id).await{Ok(v)=>windows.set(v),Err(e)=>error.set(Some(e.to_string()))}})}});
 let create=move|e:leptos::ev::SubmitEvent|{e.prevent_default();let Some(id)=selected.get()else{return};let req=monitoring::MaintenanceWindowPayload{starts_at:iso(start.get()),ends_at:iso(end.get()),reason:reason.get()};spawn_local(async move{match api::create_maintenance(&id,&req).await{Ok(v)=>{windows.update(|w|w.insert(0,v));notice.set(Some("Maintenance window created.".into()));},Err(e)=>error.set(Some(e.to_string()))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"Maintenance windows"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<section class="card"><label>"Monitor"<select on:change=move|e|selected.set(event_target_value(&e).into())><option value="">"Select monitor"</option>{move||monitors.get().into_iter().map(|m|view!{<option value={m.id}>{m.name}</option>}).collect_view()}</select></label></section>{move||selected.get().map(|_|view!{<section class="card"><h2>"Create window"</h2><form on:submit=create class="stack-form"><label>"Starts"<input type="datetime-local" prop:value=start on:input=move|e|start.set(event_target_value(&e)) required/></label><label>"Ends"<input type="datetime-local" prop:value=end on:input=move|e|end.set(event_target_value(&e)) required/></label><label>"Reason"<textarea prop:value=reason on:input=move|e|reason.set(event_target_value(&e)) maxlength="500"/></label><button class="btn btn-primary">"Create"</button></form><h2>"Configured windows"</h2>{move||windows.get().into_iter().map(|w|view!{<div class="list-row"><div><strong>{w.reason}</strong><div class="muted">{w.starts_at} → {w.ends_at}</div></div></div>}).collect_view()}</section>})}<p class="muted">"Maintenance windows suppress notifications while checks continue to run and incidents remain recorded."</p></div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/mod.rs")"
cat > "apps/leptos-dashboard/src/pages/mod.rs" <<'EOF_PINGVERTEX'
pub mod dashboard_overview;
pub mod forgot_password;
pub mod login;
pub mod logout;
pub mod monitor_detail;
pub mod monitor_form;
pub mod monitors;
pub mod not_found;
pub mod register;
pub mod reset_password;
pub mod rum_dashboard;
pub mod status;
pub mod status_pages;
pub mod team;
pub mod invitations;
pub mod api_keys;
pub mod api_flows;
pub mod billing;
pub mod pricing;
pub mod add_ons;
pub mod help_center;
pub mod support;
pub mod maintenance;
pub mod escalation;
pub mod rum_sites;
pub mod account;
pub mod notifications;
pub mod security;
pub mod verify_email;
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/monitor_detail.rs")"
cat > "apps/leptos-dashboard/src/pages/monitor_detail.rs" <<'EOF_PINGVERTEX'
use crate::api::monitoring::{self, ChannelPayload, Check, Incident, Monitor, NotificationChannel};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::empty_state::EmptyState;
use crate::components::metric_card::MetricCard;
use crate::components::page_header::PageHeader;
use crate::components::status_badge::StatusBadge;
use crate::components::table_shell::TableShell;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};

#[component]
pub fn MonitorDetailPage() -> impl IntoView {
    let params = use_params_map();
    let monitor_id = move || params.get().get("id").unwrap_or_default();
    let monitor = RwSignal::new(Monitor::default());
    let checks = RwSignal::new(Vec::<Check>::new());
    let incidents = RwSignal::new(Vec::<Incident>::new());
    let channels = RwSignal::new(Vec::<NotificationChannel>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    let load_data = move || {
        let id = monitor_id();
        if id.is_empty() {
            return;
        }
        spawn_local(async move {
            let monitor_fut = monitoring::get_monitor(&id);
            let checks_fut = monitoring::fetch_checks(&id);
            let incidents_fut = monitoring::fetch_incidents(&id);
            let channels_fut = monitoring::list_notification_channels(&id);

            match monitor_fut.await {
                Ok(m) => monitor.set(m),
                Err(e) => error.set(Some(e)),
            }
            match checks_fut.await {
                Ok(c) => checks.set(c),
                Err(e) => error.set(Some(e)),
            }
            match incidents_fut.await {
                Ok(i) => incidents.set(i),
                Err(e) => error.set(Some(e)),
            }
            match channels_fut.await {
                Ok(ch) => channels.set(ch),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    Effect::new(move |_| {
        load_data();
    });

    let show_delete_confirm = RwSignal::new(false);

    let navigate_for_delete = navigate.clone();
    let confirm_delete_cb = Callback::new(move |_: ()| {
        let id = monitor_id();
        let navigate = navigate_for_delete.clone();
        spawn_local(async move {
            if let Err(e) = monitoring::delete_monitor(&id).await {
                error.set(Some(e));
            } else {
                navigate("/monitors", Default::default());
            }
        });
        show_delete_confirm.set(false);
    });
    let cancel_delete_cb = Callback::new(move |_: ()| {
        show_delete_confirm.set(false);
    });

    let toggle_enabled = move |_| {
        let id = monitor_id();
        let current = monitor.get().enabled;
        spawn_local(async move {
            if let Err(e) = monitoring::set_monitor_enabled(&id, !current).await {
                error.set(Some(e));
            } else {
                load_data();
            }
        });
    };

    let acknowledge = move |incident_id: String| {
        spawn_local(async move {
            if let Err(e) = monitoring::acknowledge_incident(&incident_id).await {
                error.set(Some(e));
            } else {
                load_data();
            }
        });
    };

    let send_test = move |channel_id: String| {
        spawn_local(async move {
            if let Err(e) = monitoring::test_notification_channel(&channel_id).await {
                error.set(Some(e));
            }
        });
    };

    let ch_name = RwSignal::new(String::new());
    let ch_kind = RwSignal::new(String::new());
    let ch_target = RwSignal::new(String::new());
    let channel_error = RwSignal::new(None::<String>);
    let channel_secret = RwSignal::new(None::<String>);

    let create_channel = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let id = monitor_id();
        let payload = ChannelPayload {
            name: ch_name.get(),
            kind: ch_kind.get(),
            target: ch_target.get(),
            chat_id: None,
        };
        spawn_local(async move {
            match monitoring::create_notification_channel(&id, &payload).await {
                Ok(created) => {
                    channel_secret.set(created.signing_secret);
                    ch_name.set(String::new());
                    ch_kind.set(String::new());
                    ch_target.set(String::new());
                    load_data();
                }
                Err(e) => channel_error.set(Some(e)),
            }
        });
    };

    view! {
        <div class="monitor-detail">
            {move || {
                if loading.get() {
                    view! { <div class="loading">"Loading monitor..."</div> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <div class="error">{err}</div> }.into_any()
                } else {
                    let m = monitor.get();
                    let navigate_edit = navigate.clone();
                    let checks_count = checks.get().len();
                    let incidents_count = incidents.get().len();
                    let uptime_display = m
                        .uptime_percentage
                        .map(|v| format!("{v:.2}%"))
                        .unwrap_or_else(|| "—".to_string());
                    let avg_response_display = m
                        .avg_response_time_ms
                        .map(|v| format!("{v:.0} ms"))
                        .unwrap_or_else(|| "—".to_string());

                    view! {
                        <PageHeader title=m.name.clone()>
                            <button on:click=move |_| navigate_edit(&format!("/monitors/{}/edit", monitor_id()), Default::default())>
                                "Edit"
                            </button>
                            <button on:click=toggle_enabled>
                                {if m.enabled { "Disable" } else { "Enable" }}
                            </button>
                            <button class="btn btn-danger" on:click=move |_| show_delete_confirm.set(true)>
                                "Delete"
                            </button>
                        </PageHeader>

                        {move || {
                            if show_delete_confirm.get() {
                                view! {
                                    <ConfirmDialog
                                        title="Delete monitor?"
                                        body="This will permanently delete this monitor and its history. This cannot be undone."
                                        on_confirm=confirm_delete_cb
                                        on_cancel=cancel_delete_cb
                                    />
                                }
                                    .into_any()
                            } else {
                                ().into_any()
                            }
                        }}

                        <div class="monitor-info">
                            <p><strong>"Kind: "</strong>{m.kind.clone()}</p>
                            <p><strong>"Status: "</strong>{m.status.clone().unwrap_or_else(|| "unknown".to_string())}</p>
                            <p><strong>"URL: "</strong>{m.url.clone().unwrap_or_else(|| "—".into())}</p>
                            <p><strong>"Host: "</strong>{m.host.clone().unwrap_or_else(|| "—".into())}</p>
                            <p><strong>"Port: "</strong>{m.port.map(|v| v.to_string()).unwrap_or_else(|| "—".into())}</p>
                            <p><strong>"Interval (s): "</strong>{m.interval_seconds}</p>
                            <p><strong>"Timeout (s): "</strong>{m.timeout_seconds}</p>
                        </div>

                        <div class="metric-strip">
                            <MetricCard label="Uptime" value=uptime_display/>
                            <MetricCard label="Avg Response" value=avg_response_display/>
                            <MetricCard label="Checks" value=checks_count.to_string()/>
                            <MetricCard label="Incidents" value=incidents_count.to_string()/>
                        </div>

                        <h2>"Recent Checks"</h2>
                        {move || {
                            if checks.get().is_empty() {
                                view! {
                                    <EmptyState
                                        title="No checks recorded"
                                        description="Checks will appear here once this monitor runs."
                                    />
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Checked At".to_string(),
                                        "Status".to_string(),
                                        "Response Time".to_string(),
                                        "Region".to_string(),
                                    ]>
                                        {checks
                                            .get()
                                            .into_iter()
                                            .take(20)
                                            .map(|c| {
                                                view! {
                                                    <tr>
                                                        <td>{c.checked_at}</td>
                                                        <td><StatusBadge status=c.status/></td>
                                                        <td>
                                                            {c
                                                                .response_time_ms
                                                                .map(|v| format!("{v} ms"))
                                                                .unwrap_or_else(|| "—".into())}
                                                        </td>
                                                        <td>{c.region.clone().unwrap_or_default()}</td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </TableShell>
                                }
                                    .into_any()
                            }
                        }}

                        <h2>"Incidents"</h2>
                        {move || {
                            if incidents.get().is_empty() {
                                view! {
                                    <EmptyState title="No incidents" description="This monitor has no recorded incidents."/>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Started".to_string(),
                                        "Title".to_string(),
                                        "Status".to_string(),
                                        "Resolved".to_string(),
                                        "Actions".to_string(),
                                    ]>
                                        {incidents
                                            .get()
                                            .into_iter()
                                            .map(|inc| {
                                                let inc_id = inc.id.clone();
                                                let status_lower = inc.status.to_lowercase();
                                                let can_acknowledge = status_lower != "resolved"
                                                    && status_lower != "acknowledged";
                                                view! {
                                                    <tr>
                                                        <td>{inc.started_at}</td>
                                                        <td>{inc.title}</td>
                                                        <td><StatusBadge status=inc.status.clone()/></td>
                                                        <td>{inc.resolved_at.clone().unwrap_or_else(|| "—".into())}</td>
                                                        <td>
                                                            {if can_acknowledge {
                                                                let inc_id = inc_id.clone();
                                                                view! {
                                                                    <button on:click=move |_| acknowledge(inc_id.clone())>
                                                                        "Acknowledge"
                                                                    </button>
                                                                }
                                                                    .into_any()
                                                            } else {
                                                                ().into_any()
                                                            }}
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </TableShell>
                                }
                                    .into_any()
                            }
                        }}

                        <h2>"Notifications"</h2>
                        {move || {
                            if channels.get().is_empty() {
                                view! {
                                    <EmptyState
                                        title="No notifications configured"
                                        description="Add a notification below to be alerted when this monitor changes status."
                                    />
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Name".to_string(),
                                        "Kind".to_string(),
                                        "Target".to_string(),
                                        "Actions".to_string(),
                                    ]>
                                        {channels
                                            .get()
                                            .into_iter()
                                            .map(|ch| {
                                                let del_id = ch.id.clone();
                                                let test_id = ch.id.clone();
                                                let delete_channel = move |_| {
                                                    let del_id = del_id.clone();
                                                    spawn_local(async move {
                                                        if let Err(e) = monitoring::delete_notification_channel(&del_id).await {
                                                            error.set(Some(e));
                                                        } else {
                                                            load_data();
                                                        }
                                                    });
                                                };
                                                view! {
                                                    <tr>
                                                        <td>{ch.name}</td>
                                                        <td>{ch.kind}</td>
                                                        <td>{ch.target}</td>
                                                        <td>
                                                            <button on:click=move |_| send_test(test_id.clone())>
                                                                "Send test"
                                                            </button>
                                                            <button on:click=delete_channel>"Delete"</button>
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </TableShell>
                                }
                                    .into_any()
                            }
                        }}

                        <form on:submit=create_channel class="channel-form">
                            <input
                                type="text"
                                placeholder="Notification name"
                                prop:value=ch_name
                                on:input=move |ev| ch_name.set(event_target_value(&ev))
                                required
                            />
                            <select prop:value=ch_kind on:change=move |ev| ch_kind.set(event_target_value(&ev))>
                                <option value="">"Select type"</option>
                                <option value="email">"Email"</option>
                                <option value="webhook">"Webhook"</option>
                                <option value="sms">"SMS"</option>
                            </select>
                            <input
                                type="text"
                                placeholder="Target (email, URL, phone)"
                                prop:value=ch_target
                                on:input=move |ev| ch_target.set(event_target_value(&ev))
                                required
                            />
                            <button type="submit">"Add Notification"</button>
                        </form>
                        {move || channel_error.get().map(|e| view! { <div class="error">{e}</div> })}
                        {move || channel_secret.get().map(|secret| view! { <div class="secret-box"><strong>"Webhook signing secret — save it now"</strong><code>{secret}</code></div> })}
                    }
                        .into_any()
                }
            }}
        </div>
    }
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/monitor_form.rs")"
cat > "apps/leptos-dashboard/src/pages/monitor_form.rs" <<'EOF_PINGVERTEX'
use crate::api::monitoring::{self, MonitorPayload};
use leptos::prelude::*;use leptos::task::spawn_local;use leptos_router::hooks::{use_navigate,use_params_map};
#[component]
pub fn MonitorFormPage()->impl IntoView{
 let params=use_params_map();let id=move||params.get().get("id").unwrap_or_default();let edit=move||!id().is_empty();let name=RwSignal::new(String::new());let kind=RwSignal::new("http".into());let url=RwSignal::new(String::new());let method=RwSignal::new("GET".into());let expected=RwSignal::new("200".into());let tcp_port=RwSignal::new(String::new());let dns_type=RwSignal::new("A".into());let dns_value=RwSignal::new(String::new());let hb_interval=RwSignal::new("60".into());let hb_grace=RwSignal::new("60".into());let interval=RwSignal::new("60".into());let timeout=RwSignal::new("10".into());let public=RwSignal::new(false);let ssl=RwSignal::new(true);let ssl_days=RwSignal::new("14".into());let error=RwSignal::new(None::<String>);let navigate=use_navigate();
 Effect::new(move |_|{if edit(){let mid=id();spawn_local(async move{match monitoring::get_monitor(&mid).await{Ok(m)=>{name.set(m.name);kind.set(m.kind);url.set(m.url.unwrap_or_default());method.set("GET".into());expected.set(m.expected_status.to_string());tcp_port.set(m.tcp_port.map(|v|v.to_string()).unwrap_or_default());dns_type.set(m.dns_record_type.unwrap_or_else(||"A".into()));dns_value.set(m.dns_expected_value.unwrap_or_default());hb_interval.set(m.heartbeat_interval_seconds.unwrap_or(60).to_string());hb_grace.set(m.heartbeat_grace_period_seconds.unwrap_or(60).to_string());interval.set(m.interval_seconds.to_string());timeout.set(m.timeout_seconds.to_string());public.set(m.is_public);ssl.set(m.ssl_check_enabled);ssl_days.set(m.ssl_expiry_warning_days.to_string());},Err(e)=>error.set(Some(e))}})}});
 let submit=move|e:leptos::ev::SubmitEvent|{e.prevent_default();let payload=MonitorPayload{name:name.get(),kind:kind.get(),url:url.get(),method:method.get(),expected_status:expected.get().parse().unwrap_or(200),interval_seconds:interval.get().parse().unwrap_or(60),timeout_seconds:timeout.get().parse().unwrap_or(10),assertions:Vec::new(),is_public:public.get(),ssl_check_enabled:ssl.get(),ssl_expiry_warning_days:ssl_days.get().parse().unwrap_or(14),tcp_port:tcp_port.get().parse().ok(),dns_record_type:if kind.get()=="dns"{Some(dns_type.get())}else{None},dns_expected_value:if kind.get()=="dns"&&!dns_value.get().is_empty(){Some(dns_value.get())}else{None},heartbeat_interval_seconds:if kind.get()=="heartbeat"{hb_interval.get().parse().ok()}else{None},heartbeat_grace_period_seconds:if kind.get()=="heartbeat"{hb_grace.get().parse().ok()}else{None}};let mid=id();spawn_local(async move{let r=if edit(){monitoring::update_monitor(&mid,&payload).await}else{monitoring::create_monitor(&payload).await};match r{Ok(m)=>navigate(&format!("/monitors/{}",m.id),Default::default()),Err(e)=>error.set(Some(e))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>{move||if edit(){"Edit monitor"}else{"New monitor"}}</h1></header><form on:submit=submit class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Kind"<select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))><option value="http">"HTTP"</option><option value="tcp">"TCP"</option><option value="dns">"DNS"</option><option value="heartbeat">"Heartbeat"</option><option value="flow">"API flow"</option></select></label>{move||match kind.get().as_str(){"http"|"flow"=>view!{<><label>"URL"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label><label>"Method"<select prop:value=method on:change=move|e|method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select></label><label>"Expected status"<input type="number" min="100" max="599" prop:value=expected on:input=move|e|expected.set(event_target_value(&e))/></label></>.into_any()},"tcp"=>view!{<><label>"Host"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label><label>"TCP port"<input type="number" min="1" max="65535" prop:value=tcp_port on:input=move|e|tcp_port.set(event_target_value(&e)) required/></label></>.into_any()},"dns"=>view!{<><label>"DNS host"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label><label>"Record type"<input prop:value=dns_type on:input=move|e|dns_type.set(event_target_value(&e)) required/></label><label>"Expected value (optional)"<input prop:value=dns_value on:input=move|e|dns_value.set(event_target_value(&e))/></label></>.into_any()},"heartbeat"=>view!{<><label>"Heartbeat endpoint"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label><label>"Heartbeat interval (seconds)"<input type="number" min="1" prop:value=hb_interval on:input=move|e|hb_interval.set(event_target_value(&e)) required/></label><label>"Grace period (seconds)"<input type="number" min="1" prop:value=hb_grace on:input=move|e|hb_grace.set(event_target_value(&e)) required/></label></>.into_any()},_=>().into_any()}}<label>"Check interval (seconds)"<input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e)) required/></label><label>"Timeout (seconds)"<input type="number" min="1" prop:value=timeout on:input=move|e|timeout.set(event_target_value(&e)) required/></label><label class="check-row"><input type="checkbox" prop:checked=public on:change=move|e|public.set(event_target::<web_sys::HtmlInputElement>(&e).checked())/>"Show on public status pages"</label><label class="check-row"><input type="checkbox" prop:checked=ssl on:change=move|e|ssl.set(event_target::<web_sys::HtmlInputElement>(&e).checked())/>"Check TLS certificate expiry"</label>{move||if ssl.get(){view!{<label>"Expiry warning (days)"<input type="number" min="1" prop:value=ssl_days on:input=move|e|ssl_days.set(event_target_value(&e))/></label>}.into_any()}else{().into_any()}}<button class="btn btn-primary" type="submit">{move||if edit(){"Save changes"}else{"Create monitor"}}</button>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</form></div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/notifications.rs")"
cat > "apps/leptos-dashboard/src/pages/notifications.rs" <<'EOF_PINGVERTEX'
use crate::api::monitoring;
use leptos::prelude::*;use leptos::task::spawn_local;
#[component]
pub fn NotificationsPage()->impl IntoView{
 let monitors=RwSignal::new(Vec::<monitoring::Monitor>::new());let channels=RwSignal::new(Vec::<monitoring::NotificationChannel>::new());let selected=RwSignal::new(String::new());let kind=RwSignal::new("email".into());let target=RwSignal::new(String::new());let chat_id=RwSignal::new(String::new());let secret=RwSignal::new(None::<String>);let error=RwSignal::new(None::<String>);let notice=RwSignal::new(None::<String>);
 Effect::new(move |_|spawn_local(async move{match monitoring::fetch_monitors().await{Ok(v)=>monitors.set(v),Err(e)=>error.set(Some(e))}}));
 Effect::new(move |_|{let id=selected.get();if id.is_empty(){channels.set(Vec::new());return}spawn_local(async move{match monitoring::list_notification_channels(&id).await{Ok(v)=>channels.set(v),Err(e)=>error.set(Some(e))}})});
 let add=move|e:leptos::ev::SubmitEvent|{e.prevent_default();let id=selected.get();if id.is_empty(){error.set(Some("Select a monitor.".into()));return}let req=monitoring::ChannelPayload{name:String::new(),kind:kind.get(),target:target.get(),chat_id:if kind.get()=="telegram"{Some(chat_id.get())}else{None}};spawn_local(async move{match monitoring::create_notification_channel(&id,&req).await{Ok(v)=>{secret.set(v.signing_secret);channels.update(|c|c.push(v.channel));target.set(String::new());notice.set(Some("Notification channel created.".into()));},Err(e)=>error.set(Some(e))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"Notifications"</h1></header><p class="muted">"Notification channels are configured per monitor. This page provides a single place to manage the channels already supported by the backend."</p>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<section class="card"><label>"Monitor"<select prop:value=selected on:change=move|e|selected.set(event_target_value(&e))><option value="">"Select monitor"</option>{move||monitors.get().into_iter().map(|m|view!{<option value={m.id}>{m.name}</option>}).collect_view()}</select></label>{move||if !selected.get().is_empty(){view!{<><form on:submit=add class="inline-form"><select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))><option value="email">"Email"</option><option value="webhook">"Webhook"</option><option value="telegram">"Telegram"</option></select>{move||if kind.get()=="telegram"{view!{<input placeholder="Telegram chat ID" prop:value=chat_id on:input=move|e|chat_id.set(event_target_value(&e)) required/>}.into_any()}else{().into_any()}}<input placeholder="Target" prop:value=target on:input=move|e|target.set(event_target_value(&e)) required/><button class="btn btn-primary">"Add"</button></form><div>{move||channels.get().into_iter().map(|c|view!{<div class="list-row"><span>{format!("{} — {}",c.kind,c.target)}</span><span>{if c.enabled{"Enabled"}else{"Disabled"}}</span></div>}).collect_view()}</div></>}.into_any()}else{().into_any()}}</section></div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/pricing.rs")"
cat > "apps/leptos-dashboard/src/pages/pricing.rs" <<'EOF_PINGVERTEX'
use crate::api::billing;
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn PricingPage()->impl IntoView{
 let monitors=RwSignal::new("1".into());let interval=RwSignal::new("60".into());let retention=RwSignal::new("30".into());let quote=RwSignal::new(None::<billing::Quote>);let error=RwSignal::new(None::<String>);
 let refresh=move||{let req=billing::QuoteRequest{monitors:monitors.get().parse().unwrap_or(1),interval_seconds:interval.get().parse().unwrap_or(60),retention_days:retention.get().parse().unwrap_or(30)};spawn_local(async move{match billing::quote(&req).await{Ok(v)=>quote.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"Pricing"</h1></header><p class="muted">"Live prices below come directly from the backend pricing configuration."</p><form on:submit=move|e|{e.prevent_default();refresh()} class="inline-form"><input type="number" min="1" prop:value=monitors on:input=move|e|monitors.set(event_target_value(&e))/><input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e))/><input type="number" min="1" prop:value=retention on:input=move|e|retention.set(event_target_value(&e))/><button class="btn btn-primary">"Calculate"</button></form>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||quote.get().map(|q|view!{<section class="card"><h2>{format!("{} {} / month",q.monthly_price,q.currency)}</h2><p>{q.interval_label} · {q.retention_label}</p><p class="muted">{format!("{} checks per period",q.checks_per_period)}</p></section>})}</div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/rum_dashboard.rs")"
cat > "apps/leptos-dashboard/src/pages/rum_dashboard.rs" <<'EOF_PINGVERTEX'
use crate::api::rum;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn metric(v: Option<f64>, decimals: usize) -> String { v.map(|n| format!("{n:.prec$}", prec = decimals)).unwrap_or_else(|| "—".into()) }
#[component]
pub fn RumDashboardPage()->impl IntoView{
 let sites=RwSignal::new(Vec::<rum::RumSite>::new());let selected=RwSignal::new(None::<String>);let range=RwSignal::new("24".into());let overview=RwSignal::new(None::<rum::BackendRumOverview>);let pages=RwSignal::new(Vec::<rum::BackendPagePerformanceRow>::new());let errors=RwSignal::new(None::<rum::BackendRumErrorBreakdown>);let browsers=RwSignal::new(None::<rum::BackendBrowserDeviceBreakdown>);let geo=RwSignal::new(Vec::<rum::BackendGeoBreakdownRow>::new());let trend=RwSignal::new(Vec::<rum::BackendPerformanceTrendPoint>::new());let sessions=RwSignal::new(None::<rum::BackendSessionStats>);let error=RwSignal::new(None::<String>);let loading=RwSignal::new(false);
 Effect::new(move |_|spawn_local(async move{match rum::list_rum_sites_typed().await{Ok(v)=>{if selected.get_untracked().is_none(){selected.set(v.first().map(|s|s.id.clone()));}sites.set(v)},Err(e)=>error.set(Some(e.to_string()))}}));
 let reload=move||{let Some(site)=selected.get()else{return};let days=range.get();loading.set(true);let now=js_sys::Date::new_0();let end=now.to_iso_string().as_string().unwrap_or_default();let start_ms=now.get_time()-(days.parse::<f64>().unwrap_or(1.0)*24.0*60.0*60.0*1000.0);let start=js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(start_ms)).to_iso_string().as_string().unwrap_or_default();spawn_local(async move{let a=rum::rum_overview_typed(&site,Some(&start),Some(&end)).await;let b=rum::rum_slow_pages_typed(&site,Some(&start),Some(&end),Some(10)).await;let c=rum::rum_error_breakdown_typed(&site,Some(&start),Some(&end),Some(10)).await;let d=rum::rum_browser_device_breakdown_typed(&site,Some(&start),Some(&end)).await;let e=rum::rum_geo_breakdown_typed(&site,Some(&start),Some(&end),Some(10)).await;let f=rum::rum_performance_trend_typed(&site,Some(&start),Some(&end)).await;let g=rum::rum_session_stats_typed(&site,Some(&start),Some(&end)).await;match a{Ok(v)=>overview.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))};if let Ok(v)=b{pages.set(v)};if let Ok(v)=c{errors.set(Some(v))};if let Ok(v)=d{browsers.set(Some(v))};if let Ok(v)=e{geo.set(v)};if let Ok(v)=f{trend.set(v)};if let Ok(v)=g{sessions.set(Some(v))};loading.set(false);});};
 Effect::new(move |_|{let _=selected.get();let _=range.get();reload()});
 view!{<div class="feature-page"><header class="page-header"><h1>"Real User Monitoring"</h1><div class="page-header-actions"><a class="btn" href="/rum/sites">"Manage sites"</a></div></header><div class="filters-bar"><label>"Site"<select on:change=move|e|selected.set(event_target_value(&e).into()) prop:value=move||selected.get().unwrap_or_default()><option value="">"Select site"</option>{move||sites.get().into_iter().map(|s|view!{<option value={s.id}>{s.name}</option>}).collect_view()}</select></label><label>"Range"<select prop:value=range on:change=move|e|range.set(event_target_value(&e))><option value="1">"24 hours"</option><option value="7">"7 days"</option><option value="30">"30 days"</option></select></label></div>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||if loading.get(){view!{<div class="loading">"Loading analytics…"</div>}.into_any()}else if selected.get().is_none(){view!{<div class="empty-state"><h2>"No RUM site selected"</h2><p>Create a site to start collecting analytics.</p></div>}.into_any()}else{view!{<><section class="overview-cards">{move||overview.get().map(|o|view!{<><div class="card"><span class="card-label">"Page views"</span><span class="card-value">{o.page_view_count}</span></div><div class="card"><span class="card-label">"Sessions"</span><span class="card-value">{o.session_count}</span></div><div class="card"><span class="card-label">"Avg load"</span><span class="card-value">{metric(o.avg_load_time_ms,0)}" ms"</span></div><div class="card"><span class="card-label">"P95 load"</span><span class="card-value">{metric(o.p95_load_time_ms,0)}" ms"</span></div><div class="card"><span class="card-label">"P75 LCP"</span><span class="card-value">{metric(o.p75_lcp_ms,0)}" ms"</span></div><div class="card"><span class="card-label">"P75 INP"</span><span class="card-value">{metric(o.p75_inp_ms,0)}" ms"</span></div><div class="card"><span class="card-label">"P75 CLS"</span><span class="card-value">{metric(o.p75_cls_ms,3)}</span></div><div class="card"><span class="card-label">"Error rate"</span><span class="card-value">{o.error_rate.map(|v|format!("{:.2}%",v*100.0)).unwrap_or_else(||"—".into())}</span></div></>})}</section><section class="card"><h2>"Performance trend"</h2>{move||if trend.get().is_empty(){view!{<p class="muted">"No trend data."</p>}.into_any()}else{view!{<table><thead><tr><th>"Time"</th><th>"Views"</th><th>"Avg load"</th><th>"Avg LCP"</th></tr></thead><tbody>{trend.get().into_iter().map(|p|view!{<tr><td>{p.bucket_start}</td><td>{p.page_view_count}</td><td>{metric(p.avg_load_time_ms,0)}" ms"</td><td>{metric(p.avg_lcp_ms,0)}" ms"</td></tr>}).collect_view()}</tbody></table>}.into_any()}</section><section class="card"><h2>"Slow pages"</h2>{move||pages.get().into_iter().map(|p|view!{<div class="list-row"><span><code>{p.path}</code></span><span>{p.view_count} " views · " {metric(p.avg_load_time_ms,0)} " ms avg · " {metric(p.p95_load_time_ms,0)} " ms p95"</span></div>}).collect_view()}</section><div class="feature-grid"><section class="card"><h2>"JavaScript errors"</h2>{move||errors.get().map(|e|view!{<><p>{e.js_error_count} " JS errors · " {e.resource_error_count} " resource errors"</p>{e.top_js_errors.into_iter().map(|x|view!{<div class="list-row"><span>{x.message}</span><strong>{x.occurrence_count}</strong></div>}).collect_view()}</>})}</section><section class="card"><h2>"Browsers & devices"</h2>{move||browsers.get().map(|b|view!{<><h3>"Browsers"</h3>{b.browsers.into_iter().map(|x|view!{<div class="list-row"><span>{x.browser_name.unwrap_or_else(||"Unknown".into())}</span><strong>{x.session_count}</strong></div>}).collect_view()}<h3>"Devices"</h3>{b.devices.into_iter().map(|x|view!{<div class="list-row"><span>{x.device_type.unwrap_or_else(||"Unknown".into())}</span><strong>{x.session_count}</strong></div>}).collect_view()}</>})}</section></div><div class="feature-grid"><section class="card"><h2>"Geography"</h2>{move||geo.get().into_iter().map(|x|view!{<div class="list-row"><span>{x.country_code.unwrap_or_else(||"Unknown".into())}</span><strong>{x.session_count}</strong></div>}).collect_view()}</section><section class="card"><h2>"Sessions"</h2>{move||sessions.get().map(|s|view!{<><p>{s.session_count} " sessions"</p><p>{s.avg_page_views_per_session.map(|v|format!("{v:.2}")).unwrap_or_else(||"—".into())} " avg page views/session"</p><p>{s.avg_session_duration_secs.map(|v|format!("{v:.0} s")).unwrap_or_else(||"—".into())} " avg duration"</p></>})}</section></div></>}.into_any()}}</div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/rum_sites.rs")"
cat > "apps/leptos-dashboard/src/pages/rum_sites.rs" <<'EOF_PINGVERTEX'
use crate::api::rum;
use leptos::prelude::*;use leptos::task::spawn_local;
#[component]
pub fn RumSitesPage()->impl IntoView{
 let sites=RwSignal::new(Vec::<rum::RumSite>::new());let name=RwSignal::new(String::new());let domain=RwSignal::new(String::new());let error=RwSignal::new(None::<String>);let notice=RwSignal::new(None::<String>);
 let reload=move||spawn_local(async move{match rum::list_rum_sites_typed().await{Ok(v)=>sites.set(v),Err(e)=>error.set(Some(e.to_string()))}});reload();
 let create=move|e:leptos::ev::SubmitEvent|{e.prevent_default();let req=rum::CreateRumSitePayload{name:name.get(),domain:domain.get()};spawn_local(async move{match rum::create_rum_site_typed(&req).await{Ok(v)=>{sites.update(|s|s.push(v));name.set(String::new());domain.set(String::new());notice.set(Some("RUM site created.".into()));},Err(e)=>error.set(Some(e.to_string()))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"RUM sites"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<div class="feature-grid"><section class="card"><h2>"Create site"</h2><form on:submit=create class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Domain"<input placeholder="example.com" prop:value=domain on:input=move|e|domain.set(event_target_value(&e)) required/></label><button class="btn btn-primary">"Create"</button></form></section><section class="card"><h2>"Sites"</h2>{move||sites.get().into_iter().map(|s|view!{<article class="list-row"><div><strong>{s.name}</strong><div class="muted">{s.domain}</div></div><code>{s.site_key}</code></article>}).collect_view()}</section></div><p class="muted">"Site keys are public ingestion identifiers, not authentication secrets."</p></div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/security.rs")"
cat > "apps/leptos-dashboard/src/pages/security.rs" <<'EOF_PINGVERTEX'
use crate::api::account;
use crate::auth::api;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn SecurityPage() -> impl IntoView {
    let profile = RwSignal::new(None::<account::AccountProfile>);
    let profile_error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            match account::fetch_account_profile().await {
                Ok(user) => profile.set(Some(user)),
                Err(e) => profile_error.set(Some(e)),
            }
        });
    });

    view! {
        <div class="page-header">
            <h1>"Security"</h1>
        </div>

        {move || profile_error.get().map(|e| view! { <div class="form-error">{e}</div> })}

        <div class="data-section">
            <EmailVerificationCard profile=profile/>
        </div>
        <div class="data-section">
            <PasswordChangeCard/>
        </div>
        <div class="data-section">
            <TotpCard profile=profile/>
        </div>
        <div class="data-section">
            <div class="card">
                <h2>"Social sign-in"</h2>
                <p class="muted">"Google and Apple token-exchange endpoints exist on the backend, but this frontend has no provider client IDs or browser SDK configuration. No fake OAuth button is exposed until those deployment settings are supplied."</p>
            </div>
        </div>
    }
}

#[component]
fn EmailVerificationCard(profile: RwSignal<Option<account::AccountProfile>>) -> impl IntoView {
    let sending = RwSignal::new(false);
    let message = RwSignal::new(None::<String>);

    let on_resend = move |_| {
        let Some(user) = profile.get() else { return };
        sending.set(true);
        message.set(None);
        spawn_local(async move {
            // The backend returns the same neutral response regardless of
            // outcome (anti-enumeration) — show one neutral message here too,
            // rather than branching on Ok/Err in a way that could leak
            // account-existence info back through the UI.
            let _ = api::resend_verification(&user.email).await;
            sending.set(false);
            message.set(Some(
                "If this address needs verifying, a new email is on its way.".to_string(),
            ));
        });
    };

    view! {
        <div class="card">
            <h2>"Email verification"</h2>
            {move || match profile.get() {
                None => view! { <p class="muted">"Loading..."</p> }.into_any(),
                Some(user) if user.email_verified => view! {
                    <p>"Your email (" {user.email} ") is verified."</p>
                }.into_any(),
                Some(user) => view! {
                    <p>"Your email (" {user.email} ") is not verified yet."</p>
                    <button
                        class="btn btn-primary"
                        disabled=move || sending.get()
                        on:click=on_resend
                    >
                        {move || if sending.get() { "Sending..." } else { "Resend verification email" }}
                    </button>
                    {move || message.get().map(|m| view! { <p class="muted">{m}</p> })}
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn PasswordChangeCard() -> impl IntoView {
    let current_password = RwSignal::new(String::new());
    let new_password = RwSignal::new(String::new());
    let confirm_password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let success = RwSignal::new(false);
    let submitting = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        success.set(false);

        let current_val = current_password.get();
        let new_val = new_password.get();
        let confirm_val = confirm_password.get();

        if new_val != confirm_val {
            error.set(Some("New passwords do not match.".to_string()));
            return;
        }

        submitting.set(true);
        spawn_local(async move {
            match api::change_password(&current_val, &new_val).await {
                Ok(()) => {
                    success.set(true);
                    current_password.set(String::new());
                    new_password.set(String::new());
                    confirm_password.set(String::new());
                }
                Err(e) => error.set(Some(e)),
            }
            submitting.set(false);
        });
    };

    view! {
        <div class="card">
            <h2>"Change password"</h2>
            <form on:submit=on_submit class="auth-form">
                <div class="field">
                    <label for="current-password">"Current password"</label>
                    <input
                        type="password"
                        id="current-password"
                        prop:value=current_password
                        on:input=move |ev| current_password.set(event_target_value(&ev))
                        required
                    />
                </div>
                <div class="field">
                    <label for="new-password-security">"New password"</label>
                    <input
                        type="password"
                        id="new-password-security"
                        prop:value=new_password
                        on:input=move |ev| new_password.set(event_target_value(&ev))
                        required
                    />
                </div>
                <div class="field">
                    <label for="confirm-new-password">"Confirm new password"</label>
                    <input
                        type="password"
                        id="confirm-new-password"
                        prop:value=confirm_password
                        on:input=move |ev| confirm_password.set(event_target_value(&ev))
                        required
                    />
                </div>
                <button type="submit" class="btn btn-primary" disabled=move || submitting.get()>
                    {move || if submitting.get() { "Updating..." } else { "Update password" }}
                </button>
            </form>
            {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}
            {move || success.get().then(|| view! { <p class="muted">"Password updated."</p> })}
        </div>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TotpStep {
    Idle,
    AwaitingCode,
    ShowingBackupCodes,
    Disabling,
}

#[component]
fn TotpCard(profile: RwSignal<Option<account::AccountProfile>>) -> impl IntoView {
    let step = RwSignal::new(TotpStep::Idle);
    let otpauth_uri = RwSignal::new(String::new());
    let totp_code = RwSignal::new(String::new());
    let backup_codes = RwSignal::new(Vec::<String>::new());
    let disable_password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let on_start_enable = move |_| {
        error.set(None);
        busy.set(true);
        spawn_local(async move {
            match api::enable_totp().await {
                Ok(uri) => {
                    otpauth_uri.set(uri);
                    step.set(TotpStep::AwaitingCode);
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let on_confirm = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let code = totp_code.get();
        busy.set(true);
        spawn_local(async move {
            match api::confirm_totp(&code).await {
                Ok(codes) => {
                    backup_codes.set(codes);
                    profile.update(|p| if let Some(u)=p { u.totp_enabled = true; });
                    step.set(TotpStep::ShowingBackupCodes);
                    totp_code.set(String::new());
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let on_disable = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let password = disable_password.get();
        busy.set(true);
        spawn_local(async move {
            match api::disable_totp(&password).await {
                Ok(()) => {
                    profile.update(|p| if let Some(u)=p { u.totp_enabled = false; });
                    step.set(TotpStep::Idle);
                    disable_password.set(String::new());
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="card">
            <h2>"Two-factor authentication"</h2>

            {move || match profile.get() {
                Some(user) if user.totp_enabled => view! { <p class="muted">"2FA is enabled for this account."</p> }.into_any(),
                Some(_) => view! { <p class="muted">"2FA is currently disabled."</p> }.into_any(),
                None => view! { <p class="muted">"Loading security status…"</p> }.into_any(),
            }}

            {move || match step.get() {
                TotpStep::Idle => view! {
                    <div class="totp-actions">
                        <button class="btn btn-primary" disabled=move || busy.get() on:click=on_start_enable>
                            "Set up 2FA"
                        </button>
                        <button
                            class="btn btn-danger"
                            disabled=move || busy.get()
                            on:click=move |_| step.set(TotpStep::Disabling)
                        >
                            "Turn off 2FA"
                        </button>
                    </div>
                }.into_any(),

                TotpStep::AwaitingCode => view! {
                    <div class="totp-setup">
                        <p>"Add this to your authenticator app, then enter the 6-digit code it shows:"</p>
                        <code class="totp-uri">{otpauth_uri}</code>
                        <p class="muted">
                            "We show this as text rather than a QR code so your 2FA secret is \
                            never sent to a third-party image service — most authenticator \
                            apps accept manual/URI entry."
                        </p>
                        <form on:submit=on_confirm class="auth-form">
                            <div class="field">
                                <label for="totp-code">"6-digit code"</label>
                                <input
                                    type="text"
                                    id="totp-code"
                                    inputmode="numeric"
                                    prop:value=totp_code
                                    on:input=move |ev| totp_code.set(event_target_value(&ev))
                                    required
                                />
                            </div>
                            <button type="submit" class="btn btn-primary" disabled=move || busy.get()>
                                "Confirm and enable"
                            </button>
                        </form>
                    </div>
                }.into_any(),

                TotpStep::ShowingBackupCodes => view! {
                    <div class="totp-backup-codes">
                        <p>"Save these backup codes now — they will not be shown again:"</p>
                        <ul class="backup-code-list">
                            {backup_codes.get().into_iter().map(|c| view! { <li><code>{c}</code></li> }).collect_view()}
                        </ul>
                        <button
                            class="btn btn-primary"
                            on:click=move |_| step.set(TotpStep::Idle)
                        >
                            "I've saved these"
                        </button>
                    </div>
                }.into_any(),

                TotpStep::Disabling => view! {
                    <form on:submit=on_disable class="auth-form">
                        <p class="muted">"Confirm your current password to turn off 2FA."</p>
                        <div class="field">
                            <label for="disable-totp-password">"Current password"</label>
                            <input
                                type="password"
                                id="disable-totp-password"
                                prop:value=disable_password
                                on:input=move |ev| disable_password.set(event_target_value(&ev))
                                required
                            />
                        </div>
                        <div class="totp-actions">
                            <button type="submit" class="btn btn-danger" disabled=move || busy.get()>
                                "Turn off 2FA"
                            </button>
                            <button
                                type="button"
                                class="btn"
                                on:click=move |_| step.set(TotpStep::Idle)
                            >
                                "Cancel"
                            </button>
                        </div>
                    </form>
                }.into_any(),
            }}

            {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}
        </div>
    }
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/status_pages.rs")"
cat > "apps/leptos-dashboard/src/pages/status_pages.rs" <<'EOF_PINGVERTEX'
use crate::api::{monitoring, status_pages as api};
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;

#[component]
pub fn StatusPagesPage() -> impl IntoView {
    let pages = RwSignal::new(Vec::<api::StatusPage>::new());
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let selected = RwSignal::new(None::<String>);
    let pending_delete = RwSignal::new(None::<String>);
    
    let reload = move || {
        spawn_local(async move {
            let p = api::list().await;
            let m = api::monitors().await;
            match (p, m) { (Ok(p), Ok(m)) => { pages.set(p); monitors.set(m); error.set(None); }, (Err(e), _) | (_, Err(e)) => error.set(Some(e.to_string())) }
            loading.set(false);
        });
    };
    reload();

    view! {
        <div class="feature-page">
            <header class="page-header"><h1>"Status pages"</h1></header>
            {move || notice.get().map(|v| view!{<div class="success">{v}</div>})}
            {move || error.get().map(|v| view!{<div class="error">{v}</div>})}
            {move || if loading.get() { view!{<div class="loading">"Loading status pages…"</div>}.into_any() } else {
                view! {
                    <div class="feature-grid">
                        <section class="card"><h2>"Create status page"</h2><StatusPageForm monitors=monitors on_created=Callback::new(move |p| { pages.update(|v| v.push(p)); notice.set(Some("Status page created.".into())); }) /></section>
                        <section class="card"><h2>"Your status pages"</h2>
                            {move || pages.get().into_iter().map(|p| {
                                let id=p.id.clone(); let id2=id.clone(); let id3=id.clone(); let nav=navigate.clone();
                                view!{<article class="list-row"><div><strong>{p.name.clone()}</strong><div class="muted">{"/status/"}{p.subdomain.clone()}</div><div class="muted">{format!("{} monitor(s)", p.monitor_ids.len())}</div></div><div class="row-actions"><button class="btn" on:click=move |_| {selected.set(Some(id.clone()));}>"Manage"</button><button class="btn btn-danger" on:click=move |_| pending_delete.set(Some(id2.clone()))>"Delete"</button></div></article>}
                            }).collect_view()}
                            {move || if pages.get().is_empty() { view!{<p class="muted">"No status pages yet."</p>}.into_any() } else { ().into_any() }}
                        </section>
                    </div>
                    {move || selected.get().and_then(|id| pages.get().into_iter().find(|p| p.id==id)).map(|p| view!{<StatusPageEditor page=p monitors=monitors on_saved=Callback::new(move |p| { pages.update(|v| { if let Some(old)=v.iter_mut().find(|x| x.id==p.id) {*old=p;} }); notice.set(Some("Status page updated.".into())); }) on_close=Callback::new(move |_| selected.set(None)) />})}
                }.into_any()
            }}
            {move || pending_delete.get().map(|id| { let id2=id.clone(); view!{<ConfirmDialog title="Delete status page" body="This permanently removes the status page." on_confirm=Callback::new(move |_| { let id=id2.clone(); spawn_local(async move { match api::delete(&id).await { Ok(())=>{pages.update(|v|v.retain(|p|p.id!=id)); notice.set(Some("Status page deleted.".into()));}, Err(e)=>error.set(Some(e.to_string())) } pending_delete.set(None); }); }) on_cancel=Callback::new(move |_| pending_delete.set(None))/>}})}
        </div>
    }
}

#[component]
fn StatusPageForm(monitors: RwSignal<Vec<monitoring::Monitor>>, on_created: Callback<api::StatusPage>) -> impl IntoView {
    let name=RwSignal::new(String::new()); let subdomain=RwSignal::new(String::new()); let selected=RwSignal::new(Vec::<String>::new()); let busy=RwSignal::new(false); let error=RwSignal::new(None::<String>);
    let submit=move |ev: leptos::ev::SubmitEvent| { ev.prevent_default(); let req=api::CreateStatusPageRequest{name:name.get(),subdomain:subdomain.get(),monitor_ids:selected.get()}; busy.set(true); spawn_local(async move { match api::create(&req).await {Ok(p)=>{on_created.run(p);name.set(String::new());subdomain.set(String::new());selected.set(Vec::new());},Err(e)=>error.set(Some(e.to_string()))} busy.set(false); }); };
    view!{<form on:submit=submit class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Subdomain"<input placeholder="status.example" prop:value=subdomain on:input=move|e|subdomain.set(event_target_value(&e)) required/></label><fieldset><legend>"Monitors"</legend>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<label class="check-row"><input type="checkbox" on:change=move|e|{let checked=event_target::<web_sys::HtmlInputElement>(&e).checked();selected.update(|v|if checked{if !v.contains(&id){v.push(id.clone())}}else{v.retain(|x|x!=&id)});}/>{m.name}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get() type="submit">"Create"</button>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</form>}
}

#[component]
fn StatusPageEditor(page: api::StatusPage, monitors: RwSignal<Vec<monitoring::Monitor>>, on_saved: Callback<api::StatusPage>, on_close: Callback<()> ) -> impl IntoView {
    let domain_initial=page.custom_domain.clone(); let domain_verified=page.custom_domain_verified; let name=RwSignal::new(page.name.clone()); let color=RwSignal::new(page.primary_color.clone().unwrap_or_default()); let selected=RwSignal::new(page.monitor_ids.clone()); let domain=RwSignal::new(domain_initial.clone().unwrap_or_default()); let busy=RwSignal::new(false); let error=RwSignal::new(None::<String>);
    let id=page.id.clone(); let id2=id.clone(); let id3=id.clone(); let logo_url=page.logo_url.clone();
    let save_callback=on_saved.clone(); let domain_callback=on_saved.clone(); let remove_callback=on_saved.clone(); let upload_callback=on_saved.clone();
    let save=move|ev:leptos::ev::SubmitEvent|{ev.prevent_default();let req=api::UpdateStatusPageRequest{name:name.get(),monitor_ids:selected.get(),logo_url:logo_url.clone(),primary_color:if color.get().trim().is_empty(){None}else{Some(color.get())}};busy.set(true);spawn_local(async move{match api::update(&id,&req).await{Ok(p)=>save_callback.run(p),Err(e)=>error.set(Some(e.to_string()))}busy.set(false);});};
    let request_domain=move||{let d=domain.get(); if d.trim().is_empty(){return} busy.set(true);spawn_local(async move{match api::request_custom_domain(&id2,&d).await{Ok(p)=>domain_callback.run(p),Err(e)=>error.set(Some(e.to_string()))}busy.set(false);});};
    let remove_domain=move||{busy.set(true);spawn_local(async move{match api::remove_custom_domain(&id3).await{Ok(p)=>remove_callback.run(p),Err(e)=>error.set(Some(e.to_string()))}busy.set(false);});};
    let upload_id=page.id.clone();
    let upload=move|ev:leptos::ev::Event|{let input=event_target::<HtmlInputElement>(&ev);if let Some(file)=input.files().and_then(|f|f.get(0)){let file_name=file.name();let mime=file.type();let id=upload_id.clone();busy.set(true);spawn_local(async move{match JsFuture::from(file.array_buffer()).await{Ok(buf)=>{let bytes=js_sys::Uint8Array::new(&buf).to_vec();match api::upload_logo(&id,bytes,&file_name,&mime).await{Ok(p)=>upload_callback.run(p),Err(e)=>error.set(Some(e.to_string()))}},Err(_)=>error.set(Some("Could not read the selected image.".into()))}busy.set(false);});}};
    view!{<section class="card feature-editor"><header class="page-header"><h2>"Manage status page"</h2><button class="btn" on:click=move |_|on_close.run(())>"Close"</button></header><form on:submit=save class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Primary color"<input type="text" placeholder="#111827" prop:value=color on:input=move|e|color.set(event_target_value(&e))/></label><fieldset><legend>"Monitors"</legend>{move||monitors.get().into_iter().map(|m|{let mid=m.id.clone();let checked=selected.get().contains(&mid);view!{<label class="check-row"><input type="checkbox" prop:checked=checked on:change=move|e|{let c=event_target::<HtmlInputElement>(&e).checked();selected.update(|v|if c{if !v.contains(&mid){v.push(mid.clone())}}else{v.retain(|x|x!=&mid)});}/>{m.name}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get() type="submit">"Save"</button></form><div class="subsection"><h3>"Logo"</h3><input type="file" accept="image/*" on:change=upload/><p class="muted">"The server validates the image and enforces its upload limit."</p></div><div class="subsection"><h3>"Custom domain"</h3><p>{move||if domain_verified{"Verified".to_string()}else if page.custom_domain.is_some(){"Pending verification".to_string()}else{"Not configured".to_string()}}</p><div class="inline-form"><input placeholder="status.example.com" prop:value=domain on:input=move|e|domain.set(event_target_value(&e))/><button class="btn" type="button" disabled=move||busy.get() on:click=move|_|request_domain()>"Request"</button><button class="btn btn-danger" type="button" disabled=move||busy.get() on:click=move|_|remove_domain()>"Remove"</button></div></div>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</section>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/support.rs")"
cat > "apps/leptos-dashboard/src/pages/support.rs" <<'EOF_PINGVERTEX'
use crate::api::support;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;use leptos::task::spawn_local;
#[component]
pub fn SupportPage()->impl IntoView{
 let auth=use_auth(); let tickets=RwSignal::new(Vec::<support::Ticket>::new());let selected=RwSignal::new(None::<(support::Ticket,Vec<support::Message>)>);let subject=RwSignal::new(String::new());let category=RwSignal::new("technical_issue".into());let message=RwSignal::new(String::new());let error=RwSignal::new(None::<String>);let notice=RwSignal::new(None::<String>);
 let can_status=move||match auth.state.get(){AuthState::Authenticated(u)=>matches!(u.role,Role::Admin|Role::Owner),_=>false};
 let reload=move||spawn_local(async move{match support::tickets().await{Ok(v)=>tickets.set(v),Err(e)=>error.set(Some(e.to_string()))}});reload();
 let create=move|e:leptos::ev::SubmitEvent|{e.prevent_default();let req=support::CreateTicket{subject:subject.get(),category:category.get()};spawn_local(async move{match support::create_ticket(&req).await{Ok(v)=>{tickets.update(|t|t.insert(0,v));subject.set(String::new());notice.set(Some("Ticket created.".into()));},Err(e)=>error.set(Some(e.to_string()))}});};
 view!{<div class="feature-page"><header class="page-header"><h1>"Support"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<div class="feature-grid"><section class="card"><h2>"New ticket"</h2><form on:submit=create class="stack-form"><label>"Subject"<input prop:value=subject on:input=move|e|subject.set(event_target_value(&e)) required/></label><label>"Category"<select prop:value=category on:change=move|e|category.set(event_target_value(&e))><option value="account">"Account"</option><option value="billing">"Billing"</option><option value="monitor">"Monitor"</option><option value="notifications">"Notifications"</option><option value="api">"API"</option><option value="security">"Security"</option><option value="technical_issue">"Technical issue"</option><option value="other">"Other"</option></select></label><button class="btn btn-primary">"Create ticket"</button></form></section><section class="card"><h2>"Your tickets"</h2>{move||tickets.get().into_iter().map(|t|{let id=t.id.clone();view!{<button class="list-row link-button" on:click=move |_|{let id=id.clone();spawn_local(async move{match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}})}><span><strong>{t.ticket_number}</strong> {t.subject}</span><span>{t.status}</span></button>}}).collect_view()}</section></div>{move||selected.get().map(|(t,msgs)|{let id=t.id.clone();view!{<section class="card"><h2>{format!("{} — {}",t.ticket_number,t.subject)}</h2><p class="muted">{t.status}</p>{move||if can_status(){let id=t.id.clone();view!{<select on:change=move|e|{let status=event_target_value(&e);let id=id.clone();spawn_local(async move{match support::set_status(&id,&status).await{Ok(())=>match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))},Err(e)=>error.set(Some(e.to_string()))}})}><option value="open">"Open"</option><option value="in_progress">"In progress"</option><option value="waiting_for_customer">"Waiting for customer"</option><option value="resolved">"Resolved"</option><option value="closed">"Closed"</option></select>}.into_any()}else{().into_any()}}{msgs.into_iter().map(|m|view!{<div class="message"><div class="muted">{m.created_at}</div><p>{m.message}</p></div>}).collect_view()}<form on:submit={let id=id.clone();move|e:leptos::ev::SubmitEvent|{e.prevent_default();let text=message.get();spawn_local(async move{match support::reply(&id,&text).await{Ok(_)=>{message.set(String::new());match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}},Err(e)=>error.set(Some(e.to_string()))}})}} class="inline-form"><input placeholder="Reply" prop:value=message on:input=move|e|message.set(event_target_value(&e)) required/><button class="btn">"Reply"</button></form></section>}})}</div>}
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/team.rs")"
cat > "apps/leptos-dashboard/src/pages/team.rs" <<'EOF_PINGVERTEX'
use crate::api::team;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_manage(role: &Role) -> bool { matches!(role, Role::Admin | Role::Owner) }

#[component]
pub fn TeamPage() -> impl IntoView {
    let auth = use_auth();
    let members = RwSignal::new(Vec::<team::Member>::new());
    let error = RwSignal::new(None::<String>);
    let loading = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let new_user = RwSignal::new(String::new());
    let new_role = RwSignal::new("Member".to_string());
    let notice = RwSignal::new(None::<String>);

    let reload = move || {
        spawn_local(async move {
            match team::list_members().await {
                Ok(v) => members.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };
    reload();

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_manage(&u.role),
        _ => false,
    };

    let add = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let uid = new_user.get();
        let role = new_role.get();
        if uid.trim().is_empty() {
            error.set(Some("User ID is required.".into()));
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match team::add_member(&team::AddMemberRequest { user_id: uid, role }).await {
                Ok(v) => {
                    members.update(|m| m.push(v));
                    new_user.set(String::new());
                    notice.set(Some("Member added.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><h1>"Team members"</h1></header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            <section class="card">
                <h2>"Members"</h2>
                {move || if loading.get() {
                    view! { <div class="loading">"Loading…"</div> }.into_any()
                } else {
                    view! {
                        <table>
                            <thead><tr><th>"User ID"</th><th>"Role"</th><th>"Joined"</th><th></th></tr></thead>
                            <tbody>
                                {move || members.get().into_iter().map(|m| {
                                    let id = m.user_id.clone();
                                    let role_value = m.role.clone();
                                    view! {
                                        <tr>
                                            <td><code>{m.user_id}</code></td>
                                            <td>
                                                <select
                                                    disabled=move || !can()
                                                    prop:value=role_value
                                                    on:change=move |e| {
                                                        let role = event_target_value(&e);
                                                        let id = id.clone();
                                                        spawn_local(async move {
                                                            if let Err(e) = team::change_role(&id, &role).await {
                                                                error.set(Some(e.to_string()));
                                                            }
                                                        });
                                                    }
                                                >
                                                    <option value="Member">"Member"</option>
                                                    <option value="Admin">"Admin"</option>
                                                    <option value="Owner">"Owner"</option>
                                                    <option value="Auditor">"Auditor"</option>
                                                    <option value="Viewer">"Viewer"</option>
                                                </select>
                                            </td>
                                            <td>{m.joined_at.unwrap_or_else(|| "Pending".into())}</td>
                                            <td>
                                                {move || if can() {
                                                    let remove_id = id.clone();
                                                    view! {
                                                        <button class="btn btn-danger" on:click=move |_| {
                                                            let remove_id = remove_id.clone();
                                                            spawn_local(async move {
                                                                match team::remove_member(&remove_id).await {
                                                                    Ok(()) => members.update(|v| v.retain(|x| x.user_id != remove_id)),
                                                                    Err(e) => error.set(Some(e.to_string())),
                                                                }
                                                            });
                                                        }>{"Remove"}</button>
                                                    }.into_any()
                                                } else { ().into_any() }}
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
            </section>
            {move || if can() {
                view! {
                    <section class="card">
                        <h2>"Add existing user"</h2>
                        <p class="muted">"The backend requires the existing user's UUID; the server remains the authorization boundary."</p>
                        <form on:submit=add class="inline-form">
                            <input placeholder="User UUID" prop:value=new_user on:input=move |e| new_user.set(event_target_value(&e)) required/>
                            <select prop:value=new_role on:change=move |e| new_role.set(event_target_value(&e))>
                                <option value="Member">"Member"</option><option value="Admin">"Admin"</option><option value="Auditor">"Auditor"</option><option value="Viewer">"Viewer"</option>
                            </select>
                            <button class="btn btn-primary" disabled=move || busy.get()>"Add"</button>
                        </form>
                    </section>
                }.into_any()
            } else { ().into_any() }}
        </div>
    }
}

EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/src/routes/mod.rs")"
cat > "apps/leptos-dashboard/src/routes/mod.rs" <<'EOF_PINGVERTEX'
use crate::auth::context::provide_auth_context;
use crate::components::require_auth::RequireAuth;
use crate::pages::dashboard_overview::DashboardOverviewPage;
use crate::pages::forgot_password::ForgotPasswordPage;
use crate::pages::login::LoginPage;
use crate::pages::logout::LogoutPage;
use crate::pages::monitor_detail::MonitorDetailPage;
use crate::pages::monitor_form::MonitorFormPage;
use crate::pages::monitors::MonitorsPage;
use crate::pages::not_found::NotFoundPage;
use crate::pages::register::RegisterPage;
use crate::pages::reset_password::ResetPasswordPage;
use crate::pages::rum_dashboard::RumDashboardPage;
use crate::pages::status::{StatusPage, SubdomainStatusPage};
use crate::pages::status_pages::StatusPagesPage;
use crate::pages::team::TeamPage;
use crate::pages::invitations::InvitationsPage;
use crate::pages::api_keys::ApiKeysPage;
use crate::pages::api_flows::ApiFlowsPage;
use crate::pages::billing::BillingPage;
use crate::pages::pricing::PricingPage;
use crate::pages::add_ons::AddOnsPage;
use crate::pages::help_center::HelpCenterPage;
use crate::pages::support::SupportPage;
use crate::pages::maintenance::MaintenancePage;
use crate::pages::escalation::EscalationPage;
use crate::pages::rum_sites::RumSitesPage;
use crate::pages::account::AccountPage;
use crate::pages::notifications::NotificationsPage;
use crate::pages::security::SecurityPage;
use crate::pages::verify_email::VerifyEmailPage;
use leptos::prelude::*;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

#[component]
pub fn App() -> impl IntoView {
    provide_auth_context();

    view! {
        <Router>
            <Routes fallback=|| view! { <NotFoundPage/> }>
                <Route path=path!("/login") view=LoginPage/>
                <Route path=path!("/register") view=RegisterPage/>
                <Route path=path!("/forgot-password") view=ForgotPasswordPage/>
                <Route path=path!("/reset-password") view=ResetPasswordPage/>
                <Route path=path!("/logout") view=LogoutPage/>
                <Route path=path!("/status") view=StatusPage/>
                <Route path=path!("/status/:subdomain") view=SubdomainStatusPage/>
                <Route path=path!("/verify-email") view=VerifyEmailPage/>
                <ParentRoute path=path!("/") view=RequireAuth>
                    <Route path=path!("") view=DashboardOverviewPage/>
                    <Route path=path!("monitors") view=MonitorsPage/>
                    <Route path=path!("monitors/new") view=MonitorFormPage/>
                    <Route path=path!("monitors/:id") view=MonitorDetailPage/>
                    <Route path=path!("monitors/:id/edit") view=MonitorFormPage/>
                    <Route path=path!("rum") view=RumDashboardPage/>
                    <Route path=path!("rum/sites") view=RumSitesPage/>
                    <Route path=path!("status-pages") view=StatusPagesPage/>
                    <Route path=path!("team") view=TeamPage/>
                    <Route path=path!("team/invitations") view=InvitationsPage/>
                    <Route path=path!("developer/api-keys") view=ApiKeysPage/>
                    <Route path=path!("developer/api-flows") view=ApiFlowsPage/>
                    <Route path=path!("billing") view=BillingPage/>
                    <Route path=path!("billing/plan") view=PricingPage/>
                    <Route path=path!("billing/add-ons") view=AddOnsPage/>
                    <Route path=path!("help") view=HelpCenterPage/>
                    <Route path=path!("support") view=SupportPage/>
                    <Route path=path!("maintenance") view=MaintenancePage/>
                    <Route path=path!("escalation-policies") view=EscalationPage/>
                    <Route path=path!("settings/account") view=AccountPage/>
                    <Route path=path!("settings/security") view=SecurityPage/>
                    <Route path=path!("settings/notifications") view=NotificationsPage/>
                </ParentRoute>
            </Routes>
        </Router>
    }
}
EOF_PINGVERTEX

mkdir -p "$(dirname "apps/leptos-dashboard/style.css")"
cat > "apps/leptos-dashboard/style.css" <<'EOF_PINGVERTEX'
/* ==========================================================================
   PingVertex — design tokens
   Precision/signal aesthetic for an uptime & RUM monitoring dashboard:
   grotesk sans for UI text, monospace for data/metrics, status color used
   only where it carries meaning (up/down/degraded).
   ========================================================================== */

:root {
  --font-sans: 'Inter', ui-sans-serif, system-ui, -apple-system, sans-serif;
  --font-mono: 'IBM Plex Mono', ui-monospace, SFMono-Regular, Menlo, monospace;

  --bg: #f6f7f9;
  --surface: #ffffff;
  --surface-sunken: #f0f2f5;
  --border: #e1e4ea;
  --border-strong: #c7ccd6;
  --ink: #12151c;
  --ink-muted: #5b6472;
  --ink-faint: #8991a0;

  --accent: #4338ca;
  --accent-hover: #372fb0;
  --accent-soft: #eef0fe;

  --up: #12855a;
  --up-soft: #e5f6ef;
  --down: #c2311f;
  --down-soft: #fbeae8;
  --degraded: #9a6300;
  --degraded-soft: #fbf1dd;
  --neutral: #6b7280;
  --neutral-soft: #eef0f3;

  --radius-sm: 4px;
  --radius-md: 6px;
  --shadow-card: 0 1px 2px rgba(18, 21, 28, 0.05);

  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;
  --space-6: 32px;
  --space-7: 48px;
}

/* ---------- Reset & base ---------- */

* { margin: 0; padding: 0; box-sizing: border-box; }

html { -webkit-text-size-adjust: 100%; }

body {
  font-family: var(--font-sans);
  background-color: var(--bg);
  color: var(--ink);
  font-size: 15px;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
}

h1, h2, h3 { font-weight: 600; letter-spacing: -0.01em; color: var(--ink); }
h1 { font-size: 1.625rem; }
h2 { font-size: 1.0625rem; margin-bottom: var(--space-3); }

a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }

p { color: var(--ink-muted); }

:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration: 0.001ms !important; transition-duration: 0.001ms !important; }
}

/* ---------- Buttons & form controls (generic, so undecorated <button> tags
   across the app still look designed) ---------- */

button, .btn {
  font-family: var(--font-sans);
  font-size: 0.875rem;
  font-weight: 500;
  background-color: var(--surface);
  color: var(--ink);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  padding: 0.5rem 0.875rem;
  cursor: pointer;
  transition: background-color 0.12s ease, border-color 0.12s ease;
}

button:hover, .btn:hover { background-color: var(--surface-sunken); }
button:disabled { opacity: 0.5; cursor: not-allowed; }

.btn-primary {
  background-color: var(--accent);
  color: #fff;
  border-color: var(--accent);
}
.btn-primary:hover { background-color: var(--accent-hover); border-color: var(--accent-hover); }

.btn-danger { color: var(--down); border-color: var(--down); background: var(--down-soft); }
.btn-danger:hover { background-color: var(--down); color: #fff; }

.btn-block { width: 100%; padding: 0.625rem 1rem; }

input, select, textarea {
  font-family: var(--font-sans);
  font-size: 0.9375rem;
  color: var(--ink);
  background-color: var(--surface);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  padding: 0.5rem 0.625rem;
  width: 100%;
}

input:focus, select:focus, textarea:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

label { display: block; font-size: 0.8125rem; font-weight: 500; color: var(--ink-muted); margin-bottom: var(--space-1); }

/* ---------- App shell ---------- */

.app-shell { display: flex; flex-direction: column; min-height: 100vh; }

.app-header {
  background-color: var(--ink);
  color: #fff;
  padding: 0 var(--space-5);
  height: 56px;
  display: flex;
  align-items: center;
  gap: var(--space-6);
}

.app-brand a { color: #fff; font-weight: 700; letter-spacing: -0.01em; }

.app-nav { display: flex; gap: var(--space-4); flex: 1; }
.app-nav a { color: rgba(255, 255, 255, 0.72); font-size: 0.875rem; font-weight: 500; }
.app-nav a:hover, .app-nav a.active { color: #fff; text-decoration: none; }

.app-user { display: flex; align-items: center; gap: var(--space-3); font-size: 0.8125rem; color: rgba(255, 255, 255, 0.72); font-family: var(--font-mono); }
.app-user a { color: rgba(255, 255, 255, 0.85); font-family: var(--font-sans); }

.app-main { flex: 1; padding: var(--space-6) var(--space-5); max-width: 1180px; width: 100%; margin: 0 auto; }

.app-footer {
  background-color: var(--surface);
  border-top: 1px solid var(--border);
  color: var(--ink-faint);
  text-align: center;
  padding: var(--space-4);
  font-size: 0.8125rem;
}

/* ---------- Page header (title + primary action) ---------- */

.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  margin-bottom: var(--space-5);
  flex-wrap: wrap;
}
.page-header h1 { margin: 0; }

/* ---------- Stat cards / overview grid ---------- */

.overview-cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}

.card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-card);
  padding: var(--space-4);
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.card-label {
  font-size: 0.75rem;
  font-weight: 500;
  color: var(--ink-muted);
}

.card-value {
  font-family: var(--font-mono);
  font-size: 1.75rem;
  font-weight: 600;
  color: var(--ink);
  font-variant-numeric: tabular-nums;
}

.card-value.up { color: var(--up); }
.card-value.down { color: var(--down); }

/* ---------- Tables ---------- */

table {
  width: 100%;
  border-collapse: collapse;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  font-size: 0.875rem;
}

.monitors-table, table { margin-bottom: var(--space-5); }

th {
  text-align: left;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--ink-muted);
  background: var(--surface-sunken);
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border);
}

td {
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border);
  color: var(--ink);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

td:first-child, th:first-child { font-family: var(--font-sans); }

tr:last-child td { border-bottom: none; }
tbody tr:hover td { background-color: var(--surface-sunken); }

td button, td .btn { padding: 0.3rem 0.625rem; font-size: 0.8125rem; margin-right: var(--space-2); }

.table-responsive { overflow-x: auto; border-radius: var(--radius-md); }
.table-responsive table { border-radius: 0; border: none; }
.data-section { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-md); padding: var(--space-4); margin-bottom: var(--space-5); box-shadow: var(--shadow-card); }
.data-section.two-col { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-5); }

/* ---------- Loading / error / empty states ---------- */

.loading, .auth-loading {
  color: var(--ink-muted);
  font-size: 0.875rem;
  padding: var(--space-5) 0;
}

.error, .form-error {
  background-color: var(--down-soft);
  border: 1px solid rgba(194, 49, 31, 0.25);
  color: var(--down);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-sm);
  font-size: 0.875rem;
  margin: var(--space-3) 0;
}

.auth-success { color: var(--up); background: var(--up-soft); border: 1px solid rgba(18, 133, 90, 0.25); padding: var(--space-3) var(--space-4); border-radius: var(--radius-sm); }

.error-boundary {
  background-color: var(--down-soft);
  border: 1px solid rgba(194, 49, 31, 0.25);
  color: var(--down);
  padding: var(--space-4);
  border-radius: var(--radius-md);
  margin: var(--space-4) 0;
}

.loading-container { display: flex; align-items: center; gap: var(--space-3); color: var(--ink-muted); font-size: 0.875rem; }

.loading-spinner {
  display: inline-block;
  border: 2.5px solid var(--border);
  border-radius: 50%;
  border-top-color: var(--accent);
  width: 18px;
  height: 18px;
  animation: spin 0.8s linear infinite;
  flex-shrink: 0;
}
@keyframes spin { to { transform: rotate(360deg); } }

.skeleton { display: flex; flex-direction: column; gap: var(--space-2); }
.skeleton-line { height: 12px; border-radius: var(--radius-sm); background: linear-gradient(90deg, var(--surface-sunken) 25%, var(--border) 37%, var(--surface-sunken) 63%); background-size: 400% 100%; animation: shimmer 1.4s ease infinite; width: 100%; }
.skeleton-line.short { width: 40%; }
@keyframes shimmer { 0% { background-position: 100% 50%; } 100% { background-position: 0 50%; } }

/* ---------- Auth pages ---------- */

.auth-shell {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-5);
  background-color: var(--ink);
  background-image: radial-gradient(rgba(255, 255, 255, 0.06) 1px, transparent 1px);
  background-size: 22px 22px;
}

.auth-card {
  width: 100%;
  max-width: 380px;
  background: var(--surface);
  border-radius: var(--radius-md);
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.28);
  padding: var(--space-6) var(--space-5);
}

.auth-card-center { text-align: center; }

.auth-brand { display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-5); }
.auth-brand-name { font-weight: 700; letter-spacing: -0.01em; color: var(--ink); }

.auth-brand-mark {
  width: 20px; height: 20px;
  border-radius: 50%;
  background: var(--accent-soft);
  display: flex; align-items: center; justify-content: center;
}
.pulse-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); position: relative; }
.pulse-dot::after {
  content: "";
  position: absolute; inset: 0;
  border-radius: 50%;
  background: var(--accent);
  animation: pulse 2s ease-out infinite;
}
@keyframes pulse {
  0% { transform: scale(1); opacity: 0.6; }
  100% { transform: scale(2.6); opacity: 0; }
}

.auth-subtitle { font-size: 0.875rem; margin-bottom: var(--space-5); }

.auth-form { display: flex; flex-direction: column; gap: var(--space-4); }
.field { display: flex; flex-direction: column; }
.auth-hint { font-size: 0.875rem; color: var(--ink-muted); }

.auth-links { margin-top: var(--space-5); font-size: 0.8125rem; color: var(--ink-muted); text-align: center; }
.auth-links-sep { margin: 0 var(--space-2); color: var(--ink-faint); }

/* ---------- Monitor detail lists ---------- */

.monitor-info { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: var(--space-3) var(--space-5); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-md); padding: var(--space-4); margin-bottom: var(--space-5); box-shadow: var(--shadow-card); }
.monitor-info p { color: var(--ink); font-size: 0.875rem; }
.monitor-info strong { color: var(--ink-muted); font-weight: 500; }

.checks-list, .incidents-list, .channels-list { display: flex; flex-direction: column; gap: var(--space-2); margin-bottom: var(--space-5); }

.check-item, .incident-item, .channel-item {
  display: flex;
  gap: var(--space-4);
  align-items: center;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-3) var(--space-4);
  font-size: 0.8125rem;
  font-family: var(--font-mono);
}

.channel-item { justify-content: space-between; font-family: var(--font-sans); }
.channel-item button { margin: 0; }

.channel-form { display: flex; gap: var(--space-3); flex-wrap: wrap; align-items: flex-end; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-md); padding: var(--space-4); }
.channel-form input, .channel-form select { width: auto; flex: 1; min-width: 160px; }

/* ---------- RUM dashboard ---------- */

.filters-bar {
  display: flex;
  gap: var(--space-5);
  align-items: flex-end;
  flex-wrap: wrap;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-4);
  margin-bottom: var(--space-5);
}
.filters-bar label { display: flex; flex-direction: column; gap: var(--space-1); font-size: 0.75rem; }
.filters-bar select, .filters-bar input { width: auto; min-width: 150px; }

.chart-section { margin-bottom: var(--space-5); }
.chart-container {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-4);
  box-shadow: var(--shadow-card);
}
.chart-container svg polyline { stroke: var(--accent); }
.chart-container svg text { fill: var(--ink-faint); font-size: 12px; font-family: var(--font-mono); }

/* ---------- Public status page ---------- */

.status-page {
  min-height: 100vh;
  background-color: var(--bg);
  padding: var(--space-7) var(--space-5);
}
.status-page-inner { max-width: 720px; margin: 0 auto; }
.status-page-inner h1 { margin-bottom: var(--space-5); }

.status-hero {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-4) var(--space-5);
  margin-bottom: var(--space-6);
  box-shadow: var(--shadow-card);
}
.status-hero-label { font-weight: 600; }

.status-dot { width: 10px; height: 10px; border-radius: 50%; background: var(--neutral); flex-shrink: 0; }
[data-variant="operational"] .status-dot, .status-dot[data-variant="operational"] { background: var(--up); }
[data-variant="degraded"] .status-dot, .status-dot[data-variant="degraded"] { background: var(--degraded); }
[data-variant="outage"] .status-dot, .status-dot[data-variant="outage"] { background: var(--down); }
[data-variant="paused"] .status-dot, .status-dot[data-variant="paused"] { background: var(--ink-faint); }

.status-section { margin-bottom: var(--space-6); }
.service-list, .incident-list { list-style: none; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden; box-shadow: var(--shadow-card); }

.service-row, .incident-row { display: flex; align-items: center; justify-content: space-between; padding: var(--space-4); border-bottom: 1px solid var(--border); }
.service-row:last-child, .incident-row:last-child { border-bottom: none; }
.service-row-main { display: flex; align-items: center; gap: var(--space-3); }
.service-name, .incident-title { font-weight: 500; font-size: 0.9375rem; }
.service-url, .incident-description { font-size: 0.8125rem; color: var(--ink-muted); }
.service-uptime, .incident-date { font-family: var(--font-mono); font-size: 0.8125rem; color: var(--ink-muted); }

/* ---------- Responsive ---------- */

@media (max-width: 720px) {
  .app-header { flex-wrap: wrap; height: auto; padding: var(--space-3) var(--space-4); gap: var(--space-3); }
  .app-main { padding: var(--space-4); }
  .data-section.two-col { grid-template-columns: 1fr; }
  .monitor-info { grid-template-columns: 1fr; }
}

/* ==========================================================================
   Path 2 additions — sidebar shell, status badges, metric cards, empty/error
   states, confirm dialog, search input, filter bar, data-table convention
   ========================================================================== */

/* ---------- Sidebar app shell ---------- */

.app-shell {
  display: flex;
  flex-direction: row;
  min-height: 100vh;
}

.sidebar {
  width: 240px;
  min-width: 240px;
  background-color: var(--ink);
  color: #fff;
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  padding: var(--space-4) var(--space-3);
  position: sticky;
  top: 0;
  height: 100vh;
  overflow-y: auto;
}

.sidebar-brand {
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  padding-bottom: var(--space-4);
}

.sidebar-brand a {
  color: #fff;
  font-weight: 700;
  letter-spacing: -0.01em;
  font-size: 1.0625rem;
  text-decoration: none;
}

.sidebar-nav {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.sidebar-group {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.sidebar-group-label {
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: rgba(255, 255, 255, 0.45);
  padding: 0 var(--space-3);
  margin-bottom: var(--space-1);
}

.sidebar-group a {
  color: rgba(255, 255, 255, 0.72);
  font-size: 0.875rem;
  font-weight: 500;
  text-decoration: none;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  transition: background-color 0.12s ease, color 0.12s ease;
}

.sidebar-group a:hover {
  color: #fff;
  background-color: rgba(255, 255, 255, 0.06);
  text-decoration: none;
}

.sidebar-group a.active {
  color: #fff;
  background-color: var(--accent);
}

/* ---------- Path 1 additions — coming-soon nav items ---------- */

.sidebar-group-disabled {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  color: rgba(255, 255, 255, 0.32);
  font-size: 0.875rem;
  font-weight: 500;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  cursor: default;
  user-select: none;
}

.sidebar-soon-badge {
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 0.03em;
  text-transform: uppercase;
  color: rgba(255, 255, 255, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.16);
  border-radius: var(--radius-sm);
  padding: 1px 5px;
}

.sidebar-footer {
  border-top: 1px solid rgba(255, 255, 255, 0.08);
  padding-top: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.sidebar-user-email {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: rgba(255, 255, 255, 0.55);
  padding: 0 var(--space-3);
  word-break: break-all;
}

.sidebar-signout {
  color: rgba(255, 255, 255, 0.85);
  font-family: var(--font-sans);
  font-size: 0.8125rem;
  font-weight: 500;
  text-decoration: none;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  transition: background-color 0.12s ease, color 0.12s ease;
}

.sidebar-signout:hover {
  color: #fff;
  background-color: rgba(255, 255, 255, 0.06);
  text-decoration: none;
}

.app-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}

.app-main {
  flex: 1;
  padding: var(--space-6) var(--space-5);
  max-width: 1180px;
  width: 100%;
  margin: 0 auto;
}

/* ---------- Status badge ---------- */

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  font-size: 0.8125rem;
  font-weight: 500;
  font-family: var(--font-sans);
  padding: 0.25rem 0.625rem;
  border-radius: var(--radius-sm);
  white-space: nowrap;
}

.status-badge-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
  background: currentColor;
}

.status-badge-label {
  color: inherit;
}

.status-badge-up {
  background-color: var(--up-soft);
  color: var(--up);
}

.status-badge-down {
  background-color: var(--down-soft);
  color: var(--down);
}

.status-badge-degraded {
  background-color: var(--degraded-soft);
  color: var(--degraded);
}

.status-badge-neutral,
.status-badge-paused {
  background-color: var(--neutral-soft);
  color: var(--neutral);
}

/* ---------- Metric card ---------- */

.metric-card {
  gap: var(--space-2);
}

.card-trend {
  font-size: 0.75rem;
  font-weight: 500;
  font-family: var(--font-sans);
  color: var(--ink-muted);
}

.card-trend-up {
  color: var(--up);
}

.card-trend-down {
  color: var(--down);
}

.card-trend-flat {
  color: var(--ink-faint);
}

/* ---------- Empty state ---------- */

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: var(--space-2);
  padding: var(--space-6) var(--space-4);
  background: var(--surface);
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius-md);
  margin: var(--space-4) 0;
}

.empty-state-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--ink);
  margin: 0;
}

.empty-state-description {
  font-size: 0.875rem;
  color: var(--ink-muted);
  margin: 0;
  max-width: 420px;
}

.empty-state-actions {
  margin-top: var(--space-2);
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
  justify-content: center;
}

/* ---------- Error state (inline, not boundary) ---------- */

.error-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: var(--space-2);
  padding: var(--space-5) var(--space-4);
  background-color: var(--down-soft);
  border: 1px solid rgba(194, 49, 31, 0.25);
  border-radius: var(--radius-md);
  margin: var(--space-4) 0;
}

.error-state-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--down);
  margin: 0;
}

.error-state-description {
  font-size: 0.875rem;
  color: var(--ink-muted);
  margin: 0;
  max-width: 420px;
}

.error-state .btn {
  margin-top: var(--space-2);
}

/* ---------- Confirm dialog ---------- */

.dialog-overlay {
  position: fixed;
  inset: 0;
  background-color: rgba(18, 21, 28, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: var(--space-4);
}

.dialog-panel {
  background: var(--surface);
  border-radius: var(--radius-md);
  box-shadow: 0 16px 32px rgba(0, 0, 0, 0.24);
  padding: var(--space-5);
  max-width: 420px;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.dialog-title {
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--ink);
  margin: 0;
}

.dialog-body {
  font-size: 0.875rem;
  color: var(--ink-muted);
  margin: 0;
}

.dialog-actions {
  display: flex;
  gap: var(--space-3);
  justify-content: flex-end;
  margin-top: var(--space-2);
}

/* ---------- Search input ---------- */

.search-input {
  position: relative;
  width: 100%;
  max-width: 320px;
}

.search-input input {
  width: 100%;
  padding-left: var(--space-4);
  font-size: 0.875rem;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background-color: var(--surface);
  color: var(--ink);
}

.search-input input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

/* ---------- Filter bar ---------- */

.filter-bar {
  display: flex;
  gap: var(--space-4);
  align-items: flex-end;
  flex-wrap: wrap;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-4);
  margin-bottom: var(--space-5);
  box-shadow: var(--shadow-card);
}

.filter-bar label {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  font-size: 0.75rem;
  font-weight: 500;
  color: var(--ink-muted);
}

.filter-bar select,
.filter-bar input {
  width: auto;
  min-width: 150px;
  font-size: 0.875rem;
}

/* ---------- Data-table convention ---------- */

.data-table {
  width: 100%;
  border-collapse: collapse;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  font-size: 0.875rem;
  margin: 0;
}

.data-table th {
  text-align: left;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--ink-muted);
  background: var(--surface-sunken);
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border);
  white-space: nowrap;
}

.data-table td {
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border);
  color: var(--ink);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.data-table td:first-child,
.data-table th:first-child {
  font-family: var(--font-sans);
}

.data-table tbody tr:last-child td {
  border-bottom: none;
}

.data-table tbody tr:hover td {
  background-color: var(--surface-sunken);
}

/* ---------- Responsive updates for sidebar ---------- */

@media (max-width: 720px) {
  .app-shell {
    flex-direction: column;
  }

  .sidebar {
    width: 100%;
    min-width: 0;
    height: auto;
    position: static;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
  }

  .sidebar-brand {
    padding: 0;
    border-bottom: none;
    padding-bottom: 0;
  }

  .sidebar-nav {
    flex-direction: row;
    gap: var(--space-4);
    overflow-x: auto;
  }

  .sidebar-group {
    flex-direction: row;
    align-items: center;
    gap: var(--space-2);
  }

  .sidebar-group-label {
    display: none;
  }

  .sidebar-group a {
    white-space: nowrap;
  }

  .sidebar-footer {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    padding-top: var(--space-3);
  }

  .app-content {
    flex: 1;
  }

  .app-main {
    padding: var(--space-4);
  }
}

/* ---------- Page header actions slot (follow-up fix) ---------- */

.page-header-actions {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex-wrap: wrap;
}

/* ---------- Path 7 additions — security page (2FA, muted text) ---------- */

.muted {
  color: var(--muted, #6b7280);
  font-size: 0.875rem;
}

.totp-actions {
  display: flex;
  gap: var(--space-3);
  flex-wrap: wrap;
}

.totp-setup,
.totp-backup-codes {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.totp-uri {
  display: block;
  word-break: break-all;
  background: var(--surface-muted, #f3f4f6);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-3);
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  user-select: all;
}

.backup-code-list {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--space-2);
  list-style: none;
  padding: 0;
  margin: 0;
}

.backup-code-list code {
  display: block;
  background: var(--surface-muted, #f3f4f6);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-2);
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  text-align: center;
}

/* ---------- Frontend coverage expansion: compact, accessible feature UI ---------- */
.feature-page { display: flex; flex-direction: column; gap: var(--space-4); max-width: 1200px; }
.feature-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: var(--space-4); }
.stack-form { display: flex; flex-direction: column; gap: var(--space-3); }
.stack-form label, .stack-form fieldset { display: flex; flex-direction: column; gap: var(--space-2); }
.stack-form fieldset { border: 1px solid var(--border); padding: var(--space-3); border-radius: var(--radius-sm); }
.inline-form { display: flex; align-items: end; gap: var(--space-2); flex-wrap: wrap; }
.inline-form label { display: flex; flex-direction: column; gap: var(--space-1); }
.row-actions { display: flex; gap: var(--space-2); flex-wrap: wrap; }
.list-row { display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); padding: var(--space-3) 0; border-bottom: 1px solid var(--border); }
.list-row:last-child { border-bottom: 0; }
.check-row { display: flex !important; flex-direction: row !important; align-items: center; gap: var(--space-2); }
.check-row input { width: auto; }
.secret-box { display: flex; flex-direction: column; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--surface-muted, #f3f4f6); }
.secret-box code { user-select: all; overflow-wrap: anywhere; }
.quote-box { display: flex; flex-direction: column; gap: var(--space-1); margin-top: var(--space-3); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); }
.subsection { margin-top: var(--space-4); padding-top: var(--space-4); border-top: 1px solid var(--border); }
.feature-editor { width: 100%; }
.link-button { width: 100%; background: transparent; border: 0; text-align: left; cursor: pointer; font: inherit; }
.article-content { white-space: pre-wrap; line-height: 1.65; }
.message { padding: var(--space-3); border-left: 3px solid var(--border); margin: var(--space-3) 0; }
.details { display: grid; grid-template-columns: minmax(120px, 180px) 1fr; gap: var(--space-2) var(--space-4); }
.details dt { font-weight: 600; }
.details dd { margin: 0; overflow-wrap: anywhere; }
.empty-state { padding: var(--space-5); text-align: center; border: 1px dashed var(--border); border-radius: var(--radius-sm); }

@media (max-width: 720px) {
  .feature-grid { grid-template-columns: 1fr; }
  .details { grid-template-columns: 1fr; }
  .inline-form { align-items: stretch; flex-direction: column; }
  .inline-form > * { width: 100%; }
}
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/api/src/handlers/advanced_monitoring.rs")"
cat > "crates/api/src/handlers/advanced_monitoring.rs" <<'EOF_PINGVERTEX'
use axum::{extract::{Extension, Path, State}, Json};
use chrono::Utc;
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{EscalationPolicy, EscalationStep};

use crate::{error::ApiError, state::AppState, validation::{parse_escalation_policy_id, parse_monitor_id, parse_notification_channel_id}};

#[derive(Debug, serde::Serialize)]
pub struct MaintenanceWindowResponse {
    pub id: String,
    pub monitor_id: String,
    pub starts_at: chrono::DateTime<Utc>,
    pub ends_at: chrono::DateTime<Utc>,
    pub reason: String,
    pub created_at: chrono::DateTime<Utc>,
}
impl From<domain::MaintenanceWindow> for MaintenanceWindowResponse {
    fn from(v: domain::MaintenanceWindow) -> Self { Self { id: v.id.0.to_string(), monitor_id: v.monitor_id.0.to_string(), starts_at: v.starts_at, ends_at: v.ends_at, reason: v.reason, created_at: v.created_at } }
}

#[derive(Debug, serde::Serialize)]
pub struct EscalationStepResponse { pub delay_minutes: u32, pub channel_id: String }
#[derive(Debug, serde::Serialize)]
pub struct EscalationPolicyResponse { pub id: String, pub monitor_id: Option<String>, pub steps: Vec<EscalationStepResponse>, pub created_at: chrono::DateTime<Utc>, pub updated_at: chrono::DateTime<Utc> }
impl From<domain::EscalationPolicy> for EscalationPolicyResponse {
    fn from(v: domain::EscalationPolicy) -> Self { Self { id: v.id.0.to_string(), monitor_id: v.monitor_id.map(|id| id.0.to_string()), steps: v.steps.into_iter().map(|s| EscalationStepResponse { delay_minutes: s.delay_minutes, channel_id: s.channel_id.0.to_string() }).collect(), created_at: v.created_at, updated_at: v.updated_at } }
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateEscalationPolicyRequest { pub monitor_id: Option<String>, pub steps: Vec<crate::handlers::feature_types::EscalationStepInput> }

pub async fn list_maintenance_windows(State(state): State<AppState>, Extension(user): Extension<AuthenticatedUser>, Path(monitor_id): Path<String>) -> Result<Json<Vec<MaintenanceWindowResponse>>, ApiError> {
    let monitor_id = parse_monitor_id(&monitor_id)?;
    let monitor = state.monitor_repository.find_by_id(monitor_id).await.map_err(|e| ApiError::internal_with_source("failed to fetch monitor", e))?.ok_or_else(|| ApiError::not_found("monitor not found"))?;
    if !AuthorizationPolicy::same_tenant(&user.tenant_context(), monitor.tenant_id()) { return Err(ApiError::not_found("monitor not found")); }
    let windows = state.maintenance_window_repository.list_by_monitor(monitor_id).await.map_err(|e| ApiError::internal_with_source("failed to list maintenance windows", e))?;
    Ok(Json(windows.into_iter().map(Into::into).collect()))
}

pub async fn list_escalation_policies(State(state): State<AppState>, Extension(user): Extension<AuthenticatedUser>) -> Result<Json<Vec<EscalationPolicyResponse>>, ApiError> {
    let policies = state.escalation_policy_repository.list_for_tenant(user.tenant_context().tenant_id()).await.map_err(|e| ApiError::internal_with_source("failed to list escalation policies", e))?;
    Ok(Json(policies.into_iter().map(Into::into).collect()))
}

pub async fn update_escalation_policy(State(state): State<AppState>, Extension(user): Extension<AuthenticatedUser>, Path(id): Path<String>, Json(payload): Json<UpdateEscalationPolicyRequest>) -> Result<Json<EscalationPolicyResponse>, ApiError> {
    if !user.can(Permission::CreateMonitor) { return Err(ApiError::forbidden("you do not have permission to update escalation policies")); }
    let id = parse_escalation_policy_id(&id)?;
    let existing = state.escalation_policy_repository.find_by_id(id).await.map_err(|e| ApiError::internal_with_source("failed to fetch escalation policy", e))?.ok_or_else(|| ApiError::not_found("escalation policy not found"))?;
    if !AuthorizationPolicy::same_tenant(&user.tenant_context(), existing.tenant_id()) { return Err(ApiError::not_found("escalation policy not found")); }
    let monitor_id = match payload.monitor_id { Some(v) => { let mid=parse_monitor_id(&v)?; let monitor=state.monitor_repository.find_by_id(mid).await.map_err(|e| ApiError::internal_with_source("failed to fetch monitor", e))?.ok_or_else(|| ApiError::not_found("monitor not found"))?; if !AuthorizationPolicy::same_tenant(&user.tenant_context(), monitor.tenant_id()) { return Err(ApiError::not_found("monitor not found")); } Some(mid) }, None => None };
    if payload.steps.is_empty() || payload.steps.len() > domain::entities::escalation_policy::MAX_ESCALATION_STEPS { return Err(ApiError::bad_request("escalation policy must contain between 1 and 5 steps")); }
    let mut steps = Vec::with_capacity(payload.steps.len());
    for input in payload.steps { let channel_id=parse_notification_channel_id(&input.channel_id)?; let channel=state.notification_channel_repository.find_by_id(channel_id).await.map_err(|e| ApiError::internal_with_source("failed to fetch notification channel", e))?.ok_or_else(|| ApiError::not_found("notification channel not found"))?; if !AuthorizationPolicy::same_tenant(&user.tenant_context(), channel.tenant_id()) || (monitor_id.is_some() && channel.monitor_id != monitor_id.unwrap()) { return Err(ApiError::not_found("notification channel not found")); } steps.push(EscalationStep { delay_minutes: input.delay_minutes, channel_id }); }
    let updated = EscalationPolicy { id: existing.id, user_id: existing.user_id, tenant_id: existing.tenant_id, monitor_id, steps, created_at: existing.created_at, updated_at: Utc::now() };
    state.escalation_policy_repository.update(&updated).await.map_err(|e| ApiError::internal_with_source("failed to update escalation policy", e))?;
    Ok(Json(updated.into()))
}

EOF_PINGVERTEX

mkdir -p "$(dirname "crates/api/src/handlers/current_session.rs")"
cat > "crates/api/src/handlers/current_session.rs" <<'EOF_PINGVERTEX'
use axum::{
    Json,
    extract::{Extension, State},
};
use serde::Serialize;

use application::ports::UserRepository;
use domain::shared::types::AuthenticatedUser;

use crate::{auth_state::AuthState, error::ApiError};

#[derive(Debug, Serialize)]
pub struct CurrentSessionResponse {
    pub authenticated: bool,
    pub user: CurrentSessionUser,
}

#[derive(Debug, Serialize)]
pub struct CurrentSessionUser {
    pub id: String,
    pub email: String,
    pub username: String,
    pub email_verified: bool,
    pub tenant_id: String,
    pub role: domain::shared::types::Role,
    pub totp_enabled: bool,
}

/// `GET /api/v1/auth/me`
///
/// The server is the source of truth for browser authentication. The JWT is
/// intentionally kept in an HttpOnly cookie and is never exposed to
/// JavaScript. This endpoint validates the session through `jwt_auth` and
/// returns only the non-sensitive identity information required by the UI.
pub async fn current_session(
    State(state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<CurrentSessionResponse>, ApiError> {
    let user_id = authenticated_user.user_id();
    let user = state
        .user_repository
        .find_by_id(&user_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to load current user", err))?
        .ok_or_else(|| ApiError::unauthorized("invalid session"))?;

    Ok(Json(CurrentSessionResponse {
        authenticated: true,
        user: CurrentSessionUser {
            id: user.id().to_string(),
            email: user.email().to_string(),
            username: user.username().to_string(),
            email_verified: user.is_email_verified(),
            tenant_id: authenticated_user
                .tenant_context()
                .tenant_id()
                .0
                .to_string(),
            role: authenticated_user.role(),
            totp_enabled: user.totp_enabled(),
        },
    }))
}
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/api/src/handlers/help_center.rs")"
cat > "crates/api/src/handlers/help_center.rs" <<'EOF_PINGVERTEX'
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;

use crate::{
    error::ApiError,
    handlers::help_types::{HelpArticleResponse, HelpCategoryResponse},
    state::AppState,
};
use application::use_cases::{
    GetHelpArticleUseCase, ListHelpCategoriesUseCase, SearchHelpArticlesUseCase,
};

#[derive(Debug, Deserialize)]
pub struct ListHelpArticlesQuery {
    pub category: Option<String>,
    pub q: Option<String>,
}

/// GET /api/v1/help/categories
pub async fn list_help_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<HelpCategoryResponse>>, ApiError> {
    let use_case = ListHelpCategoriesUseCase::new(state.help_category_repository.clone());
    let categories = use_case
        .execute()
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list help categories", err))?;
    Ok(Json(
        categories
            .into_iter()
            .map(HelpCategoryResponse::from)
            .collect(),
    ))
}

/// GET /api/v1/help/articles?category=<slug>&q=<search>
pub async fn list_help_articles(
    State(state): State<AppState>,
    Query(query): Query<ListHelpArticlesQuery>,
) -> Result<Json<Vec<HelpArticleResponse>>, ApiError> {
    if let Some(q) = query.q {
        return search_help_articles_handler(state, q).await;
    }

    let category_id = match query.category {
        Some(slug) => {
            let repo = state.help_category_repository.clone();
            let cat = repo
                .find_by_slug(&slug)
                .await
                .map_err(|err| {
                    ApiError::internal_with_source("failed to resolve help category", err)
                })?
                .ok_or_else(|| ApiError::not_found("help category not found"))?;
            Some(cat.id)
        }
        None => None,
    };

    let articles = match category_id {
        Some(category_id) => state
            .help_article_repository
            .list_by_category(category_id, true)
            .await
            .map_err(|err| ApiError::internal_with_source("failed to list help articles", err))?,
        None => state
            .help_article_repository
            .list_published()
            .await
            .map_err(|err| ApiError::internal_with_source("failed to list help articles", err))?,
    };

    Ok(Json(
        articles
            .into_iter()
            .map(HelpArticleResponse::from)
            .collect(),
    ))
}

async fn search_help_articles_handler(
    state: AppState,
    q: String,
) -> Result<Json<Vec<HelpArticleResponse>>, ApiError> {
    let use_case = SearchHelpArticlesUseCase::new(state.help_article_repository.clone());
    let articles = use_case
        .execute(&q)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to search help articles", err))?;
    Ok(Json(
        articles
            .into_iter()
            .map(HelpArticleResponse::from)
            .collect(),
    ))
}

/// GET /api/v1/help/articles/:slug
pub async fn get_help_article(
    State(state): State<AppState>,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<HelpArticleResponse>, ApiError> {
    let use_case = GetHelpArticleUseCase::new(state.help_article_repository.clone());
    let article = use_case
        .execute(&slug)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch help article", err))?
        .ok_or_else(|| ApiError::not_found("help article not found"))?;
    Ok(Json(HelpArticleResponse::from(article)))
}

/// GET /api/v1/help/search?q=...
pub async fn search_help_articles(
    State(state): State<AppState>,
    Query(query): Query<ListHelpArticlesQuery>,
) -> Result<Json<Vec<HelpArticleResponse>>, ApiError> {
    let q = query.q.unwrap_or_default();
    if q.trim().is_empty() {
        return Ok(Json(Vec::new()));
    }
    search_help_articles_handler(state, q).await
}
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/api/src/handlers/mod.rs")"
cat > "crates/api/src/handlers/mod.rs" <<'EOF_PINGVERTEX'
pub mod acknowledge_incident;
pub mod api_key_types;
pub mod change_password;
pub mod common;
pub mod confirm_totp;
pub mod create_api_flow;
pub mod create_api_key;
pub mod create_escalation_policy;
pub mod create_maintenance_window;
pub mod create_monitor;
pub mod create_notification_channel;
pub mod create_status_page;
pub mod current_session;
pub mod delete_api_flow;
pub mod delete_api_key;
pub mod delete_monitor;
pub mod delete_notification_channel;
pub mod delete_status_page;
pub mod disable_monitor;
pub mod disable_totp;
pub mod enable_monitor;
pub mod enable_totp;
pub mod external_checks;
pub mod external_incidents;
pub mod external_monitors;
pub mod external_status_pages;
pub mod feature_types;
pub mod flow_types;
pub mod forgot_password;
pub mod get_api_flows;
pub mod get_monitor;
pub mod get_proxy_check_job;
pub mod get_public_status;
pub mod get_public_status_page;
pub mod health;
pub mod heartbeat;
pub mod list_api_keys;
pub mod list_checks;
pub mod list_incidents;
pub mod list_monitor_checks;
pub mod list_monitors;
pub mod list_notification_channels;
pub mod list_status_pages;
pub mod login;
pub mod logout;
pub mod oauth_apple;
pub mod oauth_google;
pub mod proxy_check_types;
pub mod public_status_page;
pub mod register;
pub mod remove_custom_domain;
pub mod request_custom_domain;
pub mod resend_verification;
pub mod reset_password;
pub mod rum_dashboard;
pub mod rum_ingest;
pub mod submit_proxy_check_job;
pub mod test_notification_channel;
pub mod types;
pub mod update_api_flow;
pub mod update_monitor;
pub mod update_status_page;
pub mod verify_email;
pub mod verify_totp;

// B8 — membership management:
pub mod add_member;
pub mod change_member_role;
pub mod list_members;
pub mod members_types;
pub mod remove_member;

// B9 — tenant invitations:
pub mod accept_invitation;
pub mod create_invitation;
pub mod invitation_types;
pub mod list_invitations;
pub mod revoke_invitation;

// H11 — Admin Pricing:
pub mod admin_pricing;
pub mod pricing_quote;
pub mod pricing_types;

// H14 — Help Center & Support:
pub mod help_center;
pub mod help_types;
pub mod support;
pub mod support_types;

pub use acknowledge_incident::*;
pub use api_key_types::*;
pub use change_password::*;
pub use create_api_flow::*;
pub use create_api_key::*;
pub use create_escalation_policy::*;
pub use create_maintenance_window::*;
pub use create_monitor::*;
pub use create_notification_channel::*;
pub use create_status_page::*;
pub use current_session::*;
pub use delete_api_flow::*;
pub use delete_api_key::*;
pub use delete_monitor::*;
pub use delete_notification_channel::*;
pub use delete_status_page::*;
pub use disable_monitor::*;
pub use enable_monitor::*;
pub use external_checks::*;
pub use external_incidents::*;
pub use external_monitors::*;
pub use external_status_pages::*;
pub use feature_types::*;
pub use forgot_password::*;
pub use get_api_flows::*;
pub use get_monitor::*;
pub use get_proxy_check_job::*;
pub use get_public_status::*;
pub use get_public_status_page::*;
pub use health::*;
pub use heartbeat::*;
pub use list_api_keys::*;
pub use list_checks::*;
pub use list_incidents::*;
pub use list_monitor_checks::*;
pub use list_monitors::*;
pub use list_notification_channels::*;
pub use list_status_pages::*;
pub use public_status_page::*;
pub use register::*;
pub use remove_custom_domain::*;
pub use request_custom_domain::*;
pub use resend_verification::*;
pub use reset_password::*;
pub use rum_dashboard::*;
pub use rum_ingest::*;
pub use submit_proxy_check_job::*;
pub use test_notification_channel::*;
pub use update_api_flow::*;
pub use update_monitor::*;
pub use update_status_page::*;
pub use verify_email::*;
pub mod upload_status_page_logo;
pub use upload_status_page_logo::*;

pub use add_member::*;
pub use change_member_role::*;
pub use list_members::*;
pub use members_types::*;
pub use remove_member::*;

pub use accept_invitation::*;
pub use create_invitation::*;
pub use invitation_types::*;
pub use list_invitations::*;
pub use revoke_invitation::*;

// B14 — Billing Provider Abstraction & Checkout:
pub mod billing_types;
pub mod initiate_checkout;
pub mod verify_checkout;
pub use billing_types::*;
pub use initiate_checkout::*;
pub use verify_checkout::*;

// B15 — Billing Webhooks:
pub mod billing_webhook;
pub use billing_webhook::*;

// H12 — Subscription Usage Update:
pub mod subscription_types;
pub mod update_subscription_usage;
pub use subscription_types::*;
pub use update_subscription_usage::*;

pub mod add_on_billing;
pub use add_on_billing::{billing_summary, create_add_on};

// H14 — Help Center & Support:
pub use help_center::{
    get_help_article, list_help_articles, list_help_categories, search_help_articles,
};
pub use help_types::{HelpArticleResponse, HelpCategoryResponse};
pub use support::{
    create_support_ticket, get_support_ticket, list_my_support_tickets, reply_to_support_ticket,
    update_support_ticket_status,
};
pub use support_types::{
    CreateSupportTicketRequest, ReplyToSupportTicketRequest, SupportMessageResponse,
    SupportTicketResponse, UpdateSupportTicketStatusRequest,
};

pub mod advanced_monitoring;
pub use advanced_monitoring::*;
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/api/src/handlers/remove_custom_domain.rs")"
cat > "crates/api/src/handlers/remove_custom_domain.rs" <<'EOF_PINGVERTEX'
use axum::{
    Json,
    extract::{Extension, Path, State},
};
use tracing::{info, warn};

use application::{commands::RemoveCustomDomain, use_cases::RemoveCustomDomainUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError, handlers::types::StatusPageResponse, state::AppState,
    validation::parse_status_page_id,
};

pub async fn remove_custom_domain(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: remove_custom_domain");

    let id = parse_status_page_id(&id)?;

    let command = RemoveCustomDomain { id };

    let use_case = RemoveCustomDomainUseCase::new(
        state.status_page_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let status_page = use_case
        .execute(command, authenticated_user)
        .await
        .map_err(|err| ApiError::forbidden_or_internal("failed to remove custom domain", err))?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    let cache_key = format!("status_page:id:{}", status_page.id.0);
    if let Err(err) = state.status_page_cache.invalidate(&cache_key).await {
        warn!(status_page_id = %status_page.id.0, error = %err, "failed to invalidate cached status page after custom-domain change");
    }

    Ok(Json(StatusPageResponse::from(status_page)))
}
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/api/src/handlers/request_custom_domain.rs")"
cat > "crates/api/src/handlers/request_custom_domain.rs" <<'EOF_PINGVERTEX'
use axum::{
    Json,
    extract::{Extension, Path, State},
};
use tracing::{info, warn};

use application::{
    commands::RequestCustomDomain,
    use_cases::{RequestCustomDomainError, RequestCustomDomainUseCase},
};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::{CustomDomainRequestBody, StatusPageResponse},
    state::AppState,
    validation::parse_status_page_id,
};

pub async fn request_custom_domain(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(payload): Json<CustomDomainRequestBody>,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: request_custom_domain");

    let id = parse_status_page_id(&id)?;

    let command = RequestCustomDomain {
        id,
        domain: payload.domain,
    };

    let use_case = RequestCustomDomainUseCase::new(
        state.status_page_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let status_page = use_case
        .execute(command, authenticated_user)
        .await
        .map_err(|err| match err {
            RequestCustomDomainError::Forbidden => {
                ApiError::forbidden("you do not have permission to manage custom domains")
            }
            RequestCustomDomainError::AlreadyClaimed { .. } => ApiError::conflict(err.to_string()),
            RequestCustomDomainError::Other(inner) => {
                ApiError::internal_with_source("failed to request custom domain", inner)
            }
        })?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    let cache_key = format!("status_page:id:{}", status_page.id.0);
    if let Err(err) = state.status_page_cache.invalidate(&cache_key).await {
        warn!(status_page_id = %status_page.id.0, error = %err, "failed to invalidate cached status page after custom-domain change");
    }

    Ok(Json(StatusPageResponse::from(status_page)))
}
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/api/src/routes/monitors.rs")"
cat > "crates/api/src/routes/monitors.rs" <<'EOF_PINGVERTEX'
use axum::{
    Router,
    routing::{get, post, put},
};

use crate::{
    handlers::{
        acknowledge_incident, advanced_monitoring::{list_maintenance_windows, list_escalation_policies, update_escalation_policy}, create_escalation_policy, create_maintenance_window, create_monitor,
        delete_monitor, disable_monitor, enable_monitor, get_monitor, list_incidents,
        list_monitor_checks, list_monitors, update_monitor,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/monitors", post(create_monitor).get(list_monitors))
        .route(
            "/monitors/{id}",
            get(get_monitor).put(update_monitor).delete(delete_monitor),
        )
        .route("/monitors/{id}/enable", post(enable_monitor))
        .route("/monitors/{id}/disable", post(disable_monitor))
        .route("/monitors/{id}/checks", get(list_monitor_checks))
        .route("/monitors/{id}/incidents", get(list_incidents))
        .route(
            "/monitors/{id}/maintenance-windows",
            post(create_maintenance_window).get(list_maintenance_windows),
        )
        .route("/escalation-policies", post(create_escalation_policy).get(list_escalation_policies))
        .route("/escalation-policies/{id}", put(update_escalation_policy))
        .route(
            "/incidents/{incident_id}/acknowledge",
            post(acknowledge_incident),
        )
}
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/api/src/state.rs")"
cat > "crates/api/src/state.rs" <<'EOF_PINGVERTEX'
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use application::ports::{
    AddOnSubscriptionRepository, AgentLeaseRepository, ApiFlowRepository, ApiKeyRepository,
    AuditLogEntryRepository, BillingAuditLogRepository, BillingWebhookIdempotencyRepository,
    CheckRepository, CheckoutDetails, CheckoutRequest, CheckoutSessionRepository,
    CustomDomainVerifier, DashboardRepository, DiagnosticReportRepository,
    EscalationPolicyRepository, GeoIpLookup, HealthCheckPort, HealthCheckResult,
    HelpArticleRepository, HelpCategoryRepository, IncidentRepository, LogoStorage,
    MaintenanceWindowRepository, MonitorRepository, NotificationChannelRepository,
    NotificationService, PasswordHasher, PaymentProvider, PaymentVerification,
    PricingConfigRepository, ProxyCheckJobRepository, RateLimiter, RateLimiterResult,
    RumAnalyticsRepository, RumEventRepository, RumSiteRepository, SecretCipher,
    SmsProviderPricing, StatusPageCache, StatusPageRepository, SubscriptionRepository,
    SubscriptionUsagePeriodRepository, SubscriptionUsageTransaction, SupportTicketRepository,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PricingAuditLogEntry;
use domain::Subscription;
use domain::entities::{ApiKey, StatusPage};
use domain::pricing::PricingConfig;
use domain::shared::ids::{ApiKeyId, CheckoutSessionId, MonitorId, StatusPageId, TenantId, UserId};
use domain::{CheckoutSession, EscalationPolicy, MaintenanceWindow};
use infra::{Argon2PasswordHasher, UuidIdGenerator};

use application::dto::DashboardOverview;

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Per-token rate limiter for `GET /public/heartbeat/{token}`. Keyed by the
/// raw token string; value is `(window_started_at, requests_in_window)`.
pub type HeartbeatRateLimiter = Arc<tokio::sync::Mutex<HashMap<String, (Instant, u32)>>>;

fn new_heartbeat_rate_limiter() -> HeartbeatRateLimiter {
    Arc::new(tokio::sync::Mutex::new(HashMap::new()))
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub monitor_repository: Arc<dyn MonitorRepository>,
    pub check_repository: Arc<dyn CheckRepository>,
    pub diagnostic_report_repository: Arc<dyn DiagnosticReportRepository>,
    pub incident_repository: Arc<dyn IncidentRepository>,
    pub notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    pub notification_service: Arc<dyn NotificationService>,
    pub api_flow_repository: Arc<dyn ApiFlowRepository>,
    pub proxy_check_job_repository: Arc<dyn ProxyCheckJobRepository>,
    pub maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
    pub escalation_policy_repository: Arc<dyn EscalationPolicyRepository>,
    pub status_page_repository: Arc<dyn StatusPageRepository>,
    pub custom_domain_verifier: Arc<dyn CustomDomainVerifier>,
    pub status_page_cache: Arc<dyn StatusPageCache>,
    pub logo_storage: Arc<dyn LogoStorage>,
    pub secret_cipher: Arc<dyn SecretCipher>,
    pub id_generator: UuidIdGenerator,
    pub heartbeat_rate_limiter: HeartbeatRateLimiter,
    pub audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    pub api_key_repository: Arc<dyn ApiKeyRepository>,
    pub password_hasher: Arc<dyn PasswordHasher>,
    pub subscription_repository: Arc<dyn SubscriptionRepository>,
    pub payment_provider: Arc<dyn PaymentProvider>,
    pub checkout_session_repository: Arc<dyn CheckoutSessionRepository>,
    pub billing_webhook_secret: Option<Arc<str>>,
    pub billing_webhook_idempotency_repository: Arc<dyn BillingWebhookIdempotencyRepository>,
    pub billing_audit_log_repository: Arc<dyn BillingAuditLogRepository>,
    pub pricing_config_repository: Arc<dyn PricingConfigRepository>,
    pub agent_lease_repository: Option<Arc<dyn AgentLeaseRepository>>,
    /// B34: backs `GET /ready`. Defaults to `NoopHealthCheckPort` (always
    /// healthy) until `with_health_check_port` substitutes a real check —
    /// see `main.rs`, which wires `infra::PostgresHealthCheck`.
    pub health_check_port: Arc<dyn HealthCheckPort>,
    /// H6 — Dashboard Overview Aggregated Backend API:
    /// Tenant-scoped, efficient aggregate queries for the dashboard.
    /// Defaults to `NoopDashboardRepository` (returns empty) until
    /// `with_dashboard_repository` substitutes the real Postgres implementation.
    pub dashboard_repository: Arc<dyn DashboardRepository>,
    /// H12 — Proration Service.
    pub subscription_usage_period_repository: Arc<dyn SubscriptionUsagePeriodRepository>,
    /// H12 — Proration Service.
    pub subscription_usage_transaction: Arc<dyn SubscriptionUsageTransaction>,
    pub add_on_subscription_repository: Arc<dyn AddOnSubscriptionRepository>,
    pub sms_provider_pricing: Arc<dyn SmsProviderPricing>,
    pub help_category_repository: Arc<dyn HelpCategoryRepository>,
    pub help_article_repository: Arc<dyn HelpArticleRepository>,
    pub support_ticket_repository: Arc<dyn SupportTicketRepository>,

    // RUM (Real User Monitoring):
    pub rum_site_repository: Arc<dyn RumSiteRepository>,
    pub rum_event_repository: Arc<dyn RumEventRepository>,
    /// Read-only aggregation queries backing the RUM dashboard (Task F).
    /// Defaults to a no-op (all-zero/empty) implementation until
    /// `.with_rum_analytics_repository(...)` wires a real one (e.g.
    /// `infra::PostgresRumAnalyticsRepository`).
    pub rum_analytics_repository: Arc<dyn RumAnalyticsRepository>,
    /// Server-side, country-only geo-IP lookup used by RUM ingestion to
    /// populate `RumSession.country_code` from the connection's IP — the
    /// browser SDK never sends geography (see Task D). Defaults to a
    /// no-op (always `None`) until `.with_geo_ip_lookup(...)` wires a
    /// real one (e.g. `infra::MaxMindGeoIpLookup`).
    pub geo_ip_lookup: Arc<dyn GeoIpLookup>,
    pub rate_limiter: Arc<dyn RateLimiter>,
}

/// Default `DashboardRepository` used until a real one is substituted in.
#[derive(Clone, Default)]
struct NoopDashboardRepository;

#[async_trait]
impl DashboardRepository for NoopDashboardRepository {
    async fn get_overview(&self, _tenant_id: TenantId) -> RepoResult<DashboardOverview> {
        Ok(DashboardOverview {
            monitor_summary: application::dto::MonitorSummary {
                total: 0,
                up: 0,
                down: 0,
                disabled: 0,
            },
            monitors: Vec::new(),
            incident_summary: application::dto::IncidentSummaryData {
                open_count: 0,
                recent: Vec::new(),
            },
        })
    }
}

/// Default `ApiKeyRepository` used until a real one is substituted in.
#[derive(Clone, Default)]
struct NoopApiKeyRepository;

#[async_trait]
impl ApiKeyRepository for NoopApiKeyRepository {
    async fn create(&self, _api_key: &ApiKey) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: ApiKeyId) -> RepoResult<Option<ApiKey>> {
        Ok(None)
    }

    async fn find_by_prefix(&self, _key_prefix: &str) -> RepoResult<Option<ApiKey>> {
        Ok(None)
    }

    async fn list_by_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<ApiKey>> {
        Ok(vec![])
    }

    async fn delete(&self, _id: ApiKeyId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn delete_for_tenant(&self, _id: ApiKeyId, _tenant_id: TenantId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn touch_last_used(&self, _id: ApiKeyId, _at: DateTime<Utc>) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopMaintenanceWindowRepository;

#[async_trait]
impl MaintenanceWindowRepository for NoopMaintenanceWindowRepository {
    async fn create(&self, _window: &MaintenanceWindow) -> RepoResult<()> {
        Ok(())
    }

    async fn list_active_by_monitor(
        &self,
        _monitor_id: MonitorId,
        _now: DateTime<Utc>,
    ) -> RepoResult<Vec<MaintenanceWindow>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
pub struct NoopLogoStorage;

#[async_trait]
impl LogoStorage for NoopLogoStorage {
    async fn store_logo(&self, _logo: &domain::LogoImage) -> RepoResult<String> {
        Err(std::io::Error::other("logo storage is not configured").into())
    }
}

#[derive(Clone, Default)]
struct NoopEscalationPolicyRepository;

#[async_trait]
impl EscalationPolicyRepository for NoopEscalationPolicyRepository {
    async fn create(&self, _policy: &EscalationPolicy) -> RepoResult<()> {
        Ok(())
    }

    async fn update(&self, _policy: &EscalationPolicy) -> RepoResult<()> {
        Ok(())
    }

    async fn find_effective(
        &self,
        _monitor_id: MonitorId,
        _tenant_id: TenantId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        Ok(None)
    }
}

#[derive(Clone, Default)]
pub struct NoopStatusPageRepository;

#[async_trait]
impl StatusPageRepository for NoopStatusPageRepository {
    async fn save(&self, _status_page: &StatusPage) -> RepoResult<()> {
        Ok(())
    }

    async fn update(&self, _status_page: &StatusPage) -> RepoResult<()> {
        Ok(())
    }

    async fn delete_for_user(&self, _id: StatusPageId, _user_id: UserId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn find_by_id_for_user(
        &self,
        _id: StatusPageId,
        _user_id: UserId,
    ) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn find_all_for_user(&self, _user_id: UserId) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }

    async fn find_by_id(&self, _id: StatusPageId) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn delete(&self, _id: StatusPageId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn find_all_for_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }

    async fn find_by_subdomain(&self, _subdomain: &str) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn find_by_verified_custom_domain(
        &self,
        _custom_domain: &str,
    ) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn find_pending_custom_domains(&self) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }

    async fn find_all_referencing_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
pub struct NoopCustomDomainVerifier;

#[async_trait]
impl CustomDomainVerifier for NoopCustomDomainVerifier {
    async fn verify(&self, _domain: &str, _expected_token: &str) -> RepoResult<bool> {
        Ok(false)
    }
}

#[derive(Clone, Default)]
pub struct NoopStatusPageCache;

#[async_trait]
impl StatusPageCache for NoopStatusPageCache {
    async fn get(&self, _key: &str) -> RepoResult<Option<String>> {
        Ok(None)
    }

    async fn set(&self, _key: &str, _value: &str, _ttl_seconds: u64) -> RepoResult<()> {
        Ok(())
    }

    async fn invalidate(&self, _key: &str) -> RepoResult<()> {
        Ok(())
    }
}

/// B34: default `HealthCheckPort` used until `with_health_check_port`
/// substitutes a real one. Always reports healthy — matches the pattern
/// every other Noop* default in this file follows (safe placeholder,
/// never the production path once `main.rs` wires the real dependency).
#[derive(Clone, Default)]
pub struct NoopHealthCheckPort;

#[async_trait]
impl HealthCheckPort for NoopHealthCheckPort {
    async fn check(&self) -> HealthCheckResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopSubscriptionRepository;

#[async_trait]
impl SubscriptionRepository for NoopSubscriptionRepository {
    async fn create(&self, _subscription: &Subscription) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_tenant_id(&self, _tenant_id: TenantId) -> RepoResult<Option<Subscription>> {
        Ok(None)
    }

    async fn update(&self, _subscription: &Subscription) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopPaymentProvider;

#[async_trait]
impl PaymentProvider for NoopPaymentProvider {
    fn provider_name(&self) -> &'static str {
        "noop"
    }

    async fn create_checkout(&self, _request: &CheckoutRequest) -> RepoResult<CheckoutDetails> {
        Err(std::io::Error::other("billing provider is not configured").into())
    }

    async fn verify_payment(&self, _provider_checkout_id: &str) -> RepoResult<PaymentVerification> {
        Err(std::io::Error::other("billing provider is not configured").into())
    }
}

#[derive(Clone, Default)]
struct NoopCheckoutSessionRepository;

#[async_trait]
impl CheckoutSessionRepository for NoopCheckoutSessionRepository {
    async fn create(&self, _session: &CheckoutSession) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: CheckoutSessionId) -> RepoResult<Option<CheckoutSession>> {
        Ok(None)
    }

    async fn update(&self, _session: &CheckoutSession) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopBillingWebhookIdempotencyRepository;

#[async_trait]
impl BillingWebhookIdempotencyRepository for NoopBillingWebhookIdempotencyRepository {
    async fn try_insert(&self, _provider_event_id: &str) -> RepoResult<bool> {
        Ok(true)
    }
}

#[derive(Clone, Default)]
struct NoopBillingAuditLogRepository;

#[async_trait]
impl BillingAuditLogRepository for NoopBillingAuditLogRepository {
    async fn create(&self, _entry: &domain::BillingAuditLogEntry) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopPricingConfigRepository;

#[async_trait]
impl PricingConfigRepository for NoopPricingConfigRepository {
    async fn find_active_config(&self) -> RepoResult<Option<PricingConfig>> {
        Ok(None)
    }

    async fn find_by_version(&self, _version: u32) -> RepoResult<Option<PricingConfig>> {
        Ok(None)
    }

    async fn create_version(
        &self,
        _config: &PricingConfig,
        _reason: Option<String>,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn record_audit_log(&self, _entry: &PricingAuditLogEntry) -> RepoResult<()> {
        Ok(())
    }

    async fn list_versions(&self) -> RepoResult<Vec<PricingConfig>> {
        Ok(vec![])
    }

    async fn list_audit_logs(&self) -> RepoResult<Vec<PricingAuditLogEntry>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopSubscriptionUsagePeriodRepository;

#[async_trait]
impl SubscriptionUsagePeriodRepository for NoopSubscriptionUsagePeriodRepository {
    async fn create_period(
        &self,
        _period: &domain::entities::SubscriptionUsagePeriod,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn close_period(
        &self,
        _period_id: domain::shared::ids::SubscriptionUsagePeriodId,
        _end_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn find_open_period(
        &self,
        _subscription_id: domain::shared::ids::SubscriptionId,
    ) -> RepoResult<Option<domain::entities::SubscriptionUsagePeriod>> {
        Ok(None)
    }

    async fn list_periods_for_cycle(
        &self,
        _subscription_id: domain::shared::ids::SubscriptionId,
        _cycle_start: DateTime<Utc>,
        _cycle_end: DateTime<Utc>,
    ) -> RepoResult<Vec<domain::entities::SubscriptionUsagePeriod>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopSubscriptionUsageTransaction;

#[async_trait]
impl SubscriptionUsageTransaction for NoopSubscriptionUsageTransaction {
    async fn change_usage(
        &self,
        _subscription: &Subscription,
        _previous_open_period_id: Option<domain::shared::ids::SubscriptionUsagePeriodId>,
        _new_period: &domain::entities::SubscriptionUsagePeriod,
    ) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopAddOnSubscriptionRepository;
#[async_trait]
impl AddOnSubscriptionRepository for NoopAddOnSubscriptionRepository {
    async fn create(&self, _: &domain::AddOnSubscription) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: domain::AddOnSubscriptionId,
    ) -> RepoResult<Option<domain::AddOnSubscription>> {
        Ok(None)
    }
    async fn list_active_by_tenant(
        &self,
        _: TenantId,
    ) -> RepoResult<Vec<domain::AddOnSubscription>> {
        Ok(vec![])
    }
    async fn list_by_tenant_and_type(
        &self,
        _: TenantId,
        _: domain::AddOnType,
    ) -> RepoResult<Vec<domain::AddOnSubscription>> {
        Ok(vec![])
    }
}
#[derive(Clone, Default)]
struct NoopSmsProviderPricing;

#[derive(Clone, Default)]
struct NoopHelpCategoryRepository;

#[async_trait]
impl HelpCategoryRepository for NoopHelpCategoryRepository {
    async fn create(&self, _: &domain::entities::HelpCategory) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: domain::shared::ids::HelpCategoryId,
    ) -> RepoResult<Option<domain::entities::HelpCategory>> {
        Ok(None)
    }
    async fn find_by_slug(&self, _: &str) -> RepoResult<Option<domain::entities::HelpCategory>> {
        Ok(None)
    }
    async fn list_all(&self) -> RepoResult<Vec<domain::entities::HelpCategory>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopHelpArticleRepository;

#[async_trait]
impl HelpArticleRepository for NoopHelpArticleRepository {
    async fn create(&self, _: &domain::entities::HelpArticle) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_slug(&self, _: &str) -> RepoResult<Option<domain::entities::HelpArticle>> {
        Ok(None)
    }
    async fn list_by_category(
        &self,
        _: domain::shared::ids::HelpCategoryId,
        _: bool,
    ) -> RepoResult<Vec<domain::entities::HelpArticle>> {
        Ok(vec![])
    }
    async fn list_published(&self) -> RepoResult<Vec<domain::entities::HelpArticle>> {
        Ok(vec![])
    }
    async fn search(&self, _: &str) -> RepoResult<Vec<domain::entities::HelpArticle>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopSupportTicketRepository;

#[async_trait]
impl SupportTicketRepository for NoopSupportTicketRepository {
    async fn create_ticket(&self, _: &domain::entities::SupportTicket) -> RepoResult<()> {
        Ok(())
    }
    async fn find_ticket(
        &self,
        _: domain::shared::ids::SupportTicketId,
    ) -> RepoResult<Option<domain::entities::SupportTicket>> {
        Ok(None)
    }
    async fn list_user_tickets(
        &self,
        _: domain::shared::ids::TenantId,
        _: domain::shared::ids::UserId,
    ) -> RepoResult<Vec<domain::entities::SupportTicket>> {
        Ok(vec![])
    }
    async fn list_tenant_tickets(
        &self,
        _: domain::shared::ids::TenantId,
    ) -> RepoResult<Vec<domain::entities::SupportTicket>> {
        Ok(vec![])
    }
    async fn create_message(&self, _: &domain::entities::SupportMessage) -> RepoResult<()> {
        Ok(())
    }
    async fn list_messages(
        &self,
        _: domain::shared::ids::SupportTicketId,
    ) -> RepoResult<Vec<domain::entities::SupportMessage>> {
        Ok(vec![])
    }
    async fn update_ticket_status(
        &self,
        _: domain::shared::ids::SupportTicketId,
        _: domain::enums::SupportTicketStatus,
    ) -> RepoResult<()> {
        Ok(())
    }
}
#[async_trait]
impl SmsProviderPricing for NoopSmsProviderPricing {
    async fn outbound_sms_unit_price(
        &self,
        _: &str,
    ) -> Result<(rust_decimal::Decimal, String), Box<dyn std::error::Error + Send + Sync>> {
        Err(std::io::Error::other("SMS provider pricing is not configured").into())
    }
}

/// Default RUM site repository until a real one (e.g.
/// `infra::PostgresRumSiteRepository`) is wired via
/// `AppState::with_rum_site_repository`. Always reports no sites found —
/// safe because `IngestRumEventUseCase` treats "unknown site" as a plain
/// rejection, never a panic or a bypass.
#[derive(Clone, Default)]
struct NoopRumSiteRepository;

#[async_trait]
impl RumSiteRepository for NoopRumSiteRepository {
    async fn create(&self, _site: &domain::RumSite) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_site_key(&self, _site_key: &str) -> RepoResult<Option<domain::RumSite>> {
        Ok(None)
    }

    async fn find_by_id(
        &self,
        _id: domain::shared::ids::RumSiteId,
        _tenant_id: TenantId,
    ) -> RepoResult<Option<domain::RumSite>> {
        Ok(None)
    }

    async fn list_by_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<domain::RumSite>> {
        Ok(vec![])
    }
}

/// Default RUM event repository until a real one (e.g.
/// `infra::PostgresRumEventRepository`) is wired via
/// `AppState::with_rum_event_repository`. Writes are silently accepted
/// and discarded — acceptable only because, paired with
/// `NoopRumSiteRepository` above, `find_by_site_key` always returns
/// `None`, so `IngestRumEventUseCase` never actually reaches these calls
/// in the unconfigured default state.
#[derive(Clone, Default)]
struct NoopRumEventRepository;

#[async_trait]
impl RumEventRepository for NoopRumEventRepository {
    async fn upsert_session(&self, session: &domain::RumSession) -> RepoResult<domain::RumSession> {
        Ok(session.clone())
    }

    async fn insert_page_view(&self, _page_view: &domain::RumPageView) -> RepoResult<()> {
        Ok(())
    }

    async fn insert_js_error(&self, _error: &domain::RumJsError) -> RepoResult<()> {
        Ok(())
    }

    async fn insert_resource_error(&self, _error: &domain::RumResourceError) -> RepoResult<()> {
        Ok(())
    }
}

/// Default rate limiter until a real distributed one (e.g.
/// `infra::RedisRateLimiter`) is wired via `AppState::with_rate_limiter`.
/// Allows every request. Intended only for tests/dev; production wiring
/// in `main.rs` MUST call `.with_rate_limiter(...)` before serving
/// traffic, since RUM ingestion has no other request-volume control of
/// its own besides the IP-based governor layer on the public router.
#[derive(Clone, Default)]
struct NoopRateLimiter;

#[async_trait]
impl RateLimiter for NoopRateLimiter {
    async fn check_and_record(
        &self,
        _namespace: &str,
        _key: &str,
        _max_requests: u32,
        _window_seconds: u64,
    ) -> RateLimiterResult<bool> {
        Ok(true)
    }
}

/// Default RUM analytics repository until a real one (e.g.
/// `infra::PostgresRumAnalyticsRepository`) is wired via
/// `AppState::with_rum_analytics_repository`. Returns all-zero/empty
/// results rather than an error, so an unconfigured dashboard shows
/// "no data yet" instead of a 500.
#[derive(Clone, Default)]
struct NoopRumAnalyticsRepository;

#[async_trait]
impl RumAnalyticsRepository for NoopRumAnalyticsRepository {
    async fn overview(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
    ) -> RepoResult<application::dto::RumOverview> {
        Ok(application::dto::RumOverview {
            page_view_count: 0,
            session_count: 0,
            avg_load_time_ms: None,
            p95_load_time_ms: None,
            p75_lcp_ms: None,
            p75_inp_ms: None,
            p75_cls: None,
            js_error_count: 0,
            resource_error_count: 0,
            error_rate: None,
        })
    }

    async fn slow_pages(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
        _limit: i64,
    ) -> RepoResult<Vec<application::dto::PagePerformanceRow>> {
        Ok(vec![])
    }

    async fn error_breakdown(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
        _top_n: i64,
    ) -> RepoResult<application::dto::RumErrorBreakdown> {
        Ok(application::dto::RumErrorBreakdown {
            js_error_count: 0,
            resource_error_count: 0,
            page_view_count: 0,
            error_rate: None,
            top_js_errors: vec![],
            top_resource_errors: vec![],
        })
    }

    async fn browser_device_breakdown(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
    ) -> RepoResult<application::dto::BrowserDeviceBreakdown> {
        Ok(application::dto::BrowserDeviceBreakdown {
            browsers: vec![],
            devices: vec![],
        })
    }

    async fn geo_breakdown(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
        _limit: i64,
    ) -> RepoResult<Vec<application::dto::GeoBreakdownRow>> {
        Ok(vec![])
    }

    async fn performance_trend(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
    ) -> RepoResult<Vec<application::dto::PerformanceTrendPoint>> {
        Ok(vec![])
    }

    async fn session_stats(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
    ) -> RepoResult<application::dto::SessionStats> {
        Ok(application::dto::SessionStats {
            session_count: 0,
            avg_page_views_per_session: None,
            avg_session_duration_secs: None,
        })
    }
}

/// Default geo-IP lookup until a real one (e.g.
/// `infra::MaxMindGeoIpLookup`) is wired via
/// `AppState::with_geo_ip_lookup`. Always reports "unknown" — RUM
/// ingestion must work identically whether or not geo-IP is configured,
/// just with `country_code = None` on every session until it is.
#[derive(Clone, Default)]
struct NoopGeoIpLookup;

impl GeoIpLookup for NoopGeoIpLookup {
    fn lookup_country_code(&self, _ip: std::net::IpAddr) -> Option<String> {
        None
    }
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new<M, C, D, I, N, S, A, P, AL>(
        monitor_repository: M,
        check_repository: C,
        diagnostic_report_repository: D,
        incident_repository: I,
        notification_channel_repository: N,
        notification_service: S,
        api_flow_repository: A,
        proxy_check_job_repository: P,
        id_generator: UuidIdGenerator,
        secret_cipher: Arc<dyn SecretCipher>,
        audit_log_entry_repository: AL,
    ) -> Self
    where
        M: MonitorRepository + 'static,
        C: CheckRepository + 'static,
        D: DiagnosticReportRepository + 'static,
        I: IncidentRepository + 'static,
        N: NotificationChannelRepository + 'static,
        S: NotificationService + 'static,
        A: ApiFlowRepository + 'static,
        P: ProxyCheckJobRepository + 'static,
        AL: AuditLogEntryRepository + 'static,
    {
        Self {
            monitor_repository: Arc::new(monitor_repository),
            check_repository: Arc::new(check_repository),
            diagnostic_report_repository: Arc::new(diagnostic_report_repository),
            incident_repository: Arc::new(incident_repository),
            notification_channel_repository: Arc::new(notification_channel_repository),
            notification_service: Arc::new(notification_service),
            api_flow_repository: Arc::new(api_flow_repository),
            proxy_check_job_repository: Arc::new(proxy_check_job_repository),
            maintenance_window_repository: Arc::new(NoopMaintenanceWindowRepository),
            escalation_policy_repository: Arc::new(NoopEscalationPolicyRepository),
            status_page_repository: Arc::new(NoopStatusPageRepository),
            custom_domain_verifier: Arc::new(NoopCustomDomainVerifier),
            status_page_cache: Arc::new(NoopStatusPageCache),
            logo_storage: Arc::new(NoopLogoStorage),
            secret_cipher,
            id_generator,
            heartbeat_rate_limiter: new_heartbeat_rate_limiter(),
            audit_log_entry_repository: Arc::new(audit_log_entry_repository),
            api_key_repository: Arc::new(NoopApiKeyRepository),
            password_hasher: Arc::new(Argon2PasswordHasher),
            subscription_repository: Arc::new(NoopSubscriptionRepository),
            payment_provider: Arc::new(NoopPaymentProvider),
            checkout_session_repository: Arc::new(NoopCheckoutSessionRepository),
            billing_webhook_secret: None,
            billing_webhook_idempotency_repository: Arc::new(
                NoopBillingWebhookIdempotencyRepository,
            ),
            billing_audit_log_repository: Arc::new(NoopBillingAuditLogRepository),
            pricing_config_repository: Arc::new(NoopPricingConfigRepository),
            agent_lease_repository: None,
            health_check_port: Arc::new(NoopHealthCheckPort),
            dashboard_repository: Arc::new(NoopDashboardRepository),
            subscription_usage_period_repository: Arc::new(NoopSubscriptionUsagePeriodRepository),
            subscription_usage_transaction: Arc::new(NoopSubscriptionUsageTransaction),
            add_on_subscription_repository: Arc::new(NoopAddOnSubscriptionRepository),
            sms_provider_pricing: Arc::new(NoopSmsProviderPricing),
            help_category_repository: Arc::new(NoopHelpCategoryRepository),
            help_article_repository: Arc::new(NoopHelpArticleRepository),
            support_ticket_repository: Arc::new(NoopSupportTicketRepository),
            rum_site_repository: Arc::new(NoopRumSiteRepository),
            rum_event_repository: Arc::new(NoopRumEventRepository),
            rum_analytics_repository: Arc::new(NoopRumAnalyticsRepository),
            geo_ip_lookup: Arc::new(NoopGeoIpLookup),
            rate_limiter: Arc::new(NoopRateLimiter),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_feature_repos<M, C, D, I, N, S, A, P, MW, EP, SP, CDV, AL>(
        monitor_repository: M,
        check_repository: C,
        diagnostic_report_repository: D,
        incident_repository: I,
        notification_channel_repository: N,
        notification_service: S,
        api_flow_repository: A,
        proxy_check_job_repository: P,
        maintenance_window_repository: MW,
        escalation_policy_repository: EP,
        status_page_repository: SP,
        custom_domain_verifier: CDV,
        status_page_cache: Arc<dyn StatusPageCache>,
        logo_storage: Arc<dyn LogoStorage>,
        id_generator: UuidIdGenerator,
        secret_cipher: Arc<dyn SecretCipher>,
        audit_log_entry_repository: AL,
    ) -> Self
    where
        M: MonitorRepository + 'static,
        C: CheckRepository + 'static,
        D: DiagnosticReportRepository + 'static,
        I: IncidentRepository + 'static,
        N: NotificationChannelRepository + 'static,
        S: NotificationService + 'static,
        A: ApiFlowRepository + 'static,
        P: ProxyCheckJobRepository + 'static,
        MW: MaintenanceWindowRepository + 'static,
        EP: EscalationPolicyRepository + 'static,
        SP: StatusPageRepository + 'static,
        CDV: CustomDomainVerifier + 'static,
        AL: AuditLogEntryRepository + 'static,
    {
        Self {
            monitor_repository: Arc::new(monitor_repository),
            check_repository: Arc::new(check_repository),
            diagnostic_report_repository: Arc::new(diagnostic_report_repository),
            incident_repository: Arc::new(incident_repository),
            notification_channel_repository: Arc::new(notification_channel_repository),
            notification_service: Arc::new(notification_service),
            api_flow_repository: Arc::new(api_flow_repository),
            proxy_check_job_repository: Arc::new(proxy_check_job_repository),
            maintenance_window_repository: Arc::new(maintenance_window_repository),
            escalation_policy_repository: Arc::new(escalation_policy_repository),
            status_page_repository: Arc::new(status_page_repository),
            custom_domain_verifier: Arc::new(custom_domain_verifier),
            status_page_cache,
            logo_storage,
            secret_cipher,
            id_generator,
            heartbeat_rate_limiter: new_heartbeat_rate_limiter(),
            audit_log_entry_repository: Arc::new(audit_log_entry_repository),
            api_key_repository: Arc::new(NoopApiKeyRepository),
            password_hasher: Arc::new(Argon2PasswordHasher),
            subscription_repository: Arc::new(NoopSubscriptionRepository),
            payment_provider: Arc::new(NoopPaymentProvider),
            checkout_session_repository: Arc::new(NoopCheckoutSessionRepository),
            billing_webhook_secret: None,
            billing_webhook_idempotency_repository: Arc::new(
                NoopBillingWebhookIdempotencyRepository,
            ),
            billing_audit_log_repository: Arc::new(NoopBillingAuditLogRepository),
            pricing_config_repository: Arc::new(NoopPricingConfigRepository),
            agent_lease_repository: None,
            health_check_port: Arc::new(NoopHealthCheckPort),
            dashboard_repository: Arc::new(NoopDashboardRepository),
            subscription_usage_period_repository: Arc::new(NoopSubscriptionUsagePeriodRepository),
            subscription_usage_transaction: Arc::new(NoopSubscriptionUsageTransaction),
            add_on_subscription_repository: Arc::new(NoopAddOnSubscriptionRepository),
            sms_provider_pricing: Arc::new(NoopSmsProviderPricing),
            help_category_repository: Arc::new(NoopHelpCategoryRepository),
            help_article_repository: Arc::new(NoopHelpArticleRepository),
            support_ticket_repository: Arc::new(NoopSupportTicketRepository),
            rum_site_repository: Arc::new(NoopRumSiteRepository),
            rum_event_repository: Arc::new(NoopRumEventRepository),
            rum_analytics_repository: Arc::new(NoopRumAnalyticsRepository),
            geo_ip_lookup: Arc::new(NoopGeoIpLookup),
            rate_limiter: Arc::new(NoopRateLimiter),
        }
    }

    pub fn with_api_key_repository<R>(mut self, api_key_repository: R) -> Self
    where
        R: ApiKeyRepository + 'static,
    {
        self.api_key_repository = Arc::new(api_key_repository);
        self
    }

    /// Substitutes a real `StatusPageRepository` in place of the
    /// `NoopStatusPageRepository` default. Mirrors `with_api_key_repository`.
    pub fn with_status_page_repository<R>(mut self, status_page_repository: R) -> Self
    where
        R: StatusPageRepository + 'static,
    {
        self.status_page_repository = Arc::new(status_page_repository);
        self
    }

    pub fn with_subscription_repository<R>(mut self, subscription_repository: R) -> Self
    where
        R: SubscriptionRepository + 'static,
    {
        self.subscription_repository = Arc::new(subscription_repository);
        self
    }

    pub fn with_payment_provider<P>(mut self, payment_provider: P) -> Self
    where
        P: PaymentProvider + 'static,
    {
        self.payment_provider = Arc::new(payment_provider);
        self
    }

    pub fn with_checkout_session_repository<R>(mut self, checkout_session_repository: R) -> Self
    where
        R: CheckoutSessionRepository + 'static,
    {
        self.checkout_session_repository = Arc::new(checkout_session_repository);
        self
    }

    pub fn with_billing_webhook_secret<S: Into<String>>(mut self, secret: S) -> Self {
        self.billing_webhook_secret = Some(Arc::from(secret.into()));
        self
    }

    pub fn with_billing_webhook_idempotency_repository<R>(mut self, repo: R) -> Self
    where
        R: BillingWebhookIdempotencyRepository + 'static,
    {
        self.billing_webhook_idempotency_repository = Arc::new(repo);
        self
    }

    pub fn with_billing_audit_log_repository<R>(mut self, repo: R) -> Self
    where
        R: BillingAuditLogRepository + 'static,
    {
        self.billing_audit_log_repository = Arc::new(repo);
        self
    }

    pub fn with_agent_lease_repository<R>(mut self, agent_lease_repository: R) -> Self
    where
        R: AgentLeaseRepository + 'static,
    {
        self.agent_lease_repository = Some(Arc::new(agent_lease_repository));
        self
    }

    /// B34: substitutes a real `HealthCheckPort` (see
    /// `infra::PostgresHealthCheck`) in place of the always-healthy
    /// `NoopHealthCheckPort` default, backing `GET /ready`.
    pub fn with_health_check_port<H>(mut self, health_check_port: H) -> Self
    where
        H: HealthCheckPort + 'static,
    {
        self.health_check_port = Arc::new(health_check_port);
        self
    }

    /// H6 — Dashboard Overview:
    /// Substitutes a real `DashboardRepository` (e.g. `PostgresDashboardRepository`)
    /// in place of the no-op default.
    pub fn with_pricing_config_repository<R>(mut self, pricing_config_repository: R) -> Self
    where
        R: PricingConfigRepository + 'static,
    {
        self.pricing_config_repository = Arc::new(pricing_config_repository);
        self
    }

    pub fn with_dashboard_repository<R>(mut self, dashboard_repository: R) -> Self
    where
        R: DashboardRepository + 'static,
    {
        self.dashboard_repository = Arc::new(dashboard_repository);
        self
    }

    /// H12 — Proration Service.
    pub fn with_subscription_usage_period_repository<R>(mut self, repo: R) -> Self
    where
        R: SubscriptionUsagePeriodRepository + 'static,
    {
        self.subscription_usage_period_repository = Arc::new(repo);
        self
    }

    /// H12 — Proration Service.
    pub fn with_add_on_subscription_repository<R>(mut self, repo: R) -> Self
    where
        R: AddOnSubscriptionRepository + 'static,
    {
        self.add_on_subscription_repository = Arc::new(repo);
        self
    }

    pub fn with_sms_provider_pricing<P>(mut self, provider: P) -> Self
    where
        P: SmsProviderPricing + 'static,
    {
        self.sms_provider_pricing = Arc::new(provider);
        self
    }

    pub fn with_subscription_usage_transaction<T>(mut self, transaction: T) -> Self
    where
        T: SubscriptionUsageTransaction + 'static,
    {
        self.subscription_usage_transaction = Arc::new(transaction);
        self
    }

    pub fn with_help_category_repository<R>(mut self, repo: R) -> Self
    where
        R: HelpCategoryRepository + 'static,
    {
        self.help_category_repository = Arc::new(repo);
        self
    }

    pub fn with_help_article_repository<R>(mut self, repo: R) -> Self
    where
        R: HelpArticleRepository + 'static,
    {
        self.help_article_repository = Arc::new(repo);
        self
    }

    pub fn with_support_ticket_repository<R>(mut self, repo: R) -> Self
    where
        R: SupportTicketRepository + 'static,
    {
        self.support_ticket_repository = Arc::new(repo);
        self
    }

    // RUM (Real User Monitoring):
    pub fn with_rum_site_repository<R>(mut self, repo: R) -> Self
    where
        R: RumSiteRepository + 'static,
    {
        self.rum_site_repository = Arc::new(repo);
        self
    }

    pub fn with_rum_event_repository<R>(mut self, repo: R) -> Self
    where
        R: RumEventRepository + 'static,
    {
        self.rum_event_repository = Arc::new(repo);
        self
    }

    pub fn with_rate_limiter<R>(mut self, rate_limiter: R) -> Self
    where
        R: RateLimiter + 'static,
    {
        self.rate_limiter = Arc::new(rate_limiter);
        self
    }

    pub fn with_rum_analytics_repository<R>(mut self, repo: R) -> Self
    where
        R: RumAnalyticsRepository + 'static,
    {
        self.rum_analytics_repository = Arc::new(repo);
        self
    }

    pub fn with_geo_ip_lookup<G>(mut self, geo_ip_lookup: G) -> Self
    where
        G: GeoIpLookup + 'static,
    {
        self.geo_ip_lookup = Arc::new(geo_ip_lookup);
        self
    }
}
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/application/src/ports/help_article_repository.rs")"
cat > "crates/application/src/ports/help_article_repository.rs" <<'EOF_PINGVERTEX'
use async_trait::async_trait;
use std::error::Error;

use domain::entities::HelpArticle;
use domain::shared::ids::HelpCategoryId;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Help Center article storage.
#[async_trait]
pub trait HelpArticleRepository: Send + Sync {
    async fn create(&self, article: &HelpArticle) -> RepoResult<()>;
    async fn find_by_slug(&self, slug: &str) -> RepoResult<Option<HelpArticle>>;
    async fn list_by_category(
        &self,
        category_id: HelpCategoryId,
        only_published: bool,
    ) -> RepoResult<Vec<HelpArticle>>;
    async fn list_published(&self) -> RepoResult<Vec<HelpArticle>>;
    async fn search(&self, query: &str) -> RepoResult<Vec<HelpArticle>>;
}

#[async_trait]
impl<T> HelpArticleRepository for std::sync::Arc<T>
where
    T: HelpArticleRepository + Send + Sync + ?Sized,
{
    async fn create(&self, article: &HelpArticle) -> RepoResult<()> {
        (**self).create(article).await
    }
    async fn find_by_slug(&self, slug: &str) -> RepoResult<Option<HelpArticle>> {
        (**self).find_by_slug(slug).await
    }
    async fn list_by_category(
        &self,
        category_id: HelpCategoryId,
        only_published: bool,
    ) -> RepoResult<Vec<HelpArticle>> {
        (**self).list_by_category(category_id, only_published).await
    }
    async fn list_published(&self) -> RepoResult<Vec<HelpArticle>> {
        (**self).list_published().await
    }
    async fn search(&self, query: &str) -> RepoResult<Vec<HelpArticle>> {
        (**self).search(query).await
    }
}
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/infra/src/repositories/in_memory_help_article_repository.rs")"
cat > "crates/infra/src/repositories/in_memory_help_article_repository.rs" <<'EOF_PINGVERTEX'
use application::ports::HelpArticleRepository;
use async_trait::async_trait;
use domain::entities::HelpArticle;
use domain::enums::HelpArticleStatus;
use domain::shared::ids::HelpCategoryId;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

type R<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Default)]
pub struct InMemoryHelpArticleRepository {
    items: Arc<Mutex<HashMap<String, HelpArticle>>>,
}

impl InMemoryHelpArticleRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_article(article: HelpArticle) -> Self {
        let repo = Self::new();
        repo.items
            .lock()
            .unwrap()
            .insert(article.slug.clone(), article);
        repo
    }
}

#[async_trait]
impl HelpArticleRepository for InMemoryHelpArticleRepository {
    async fn create(&self, article: &HelpArticle) -> R<()> {
        self.items
            .lock()
            .unwrap()
            .insert(article.slug.clone(), article.clone());
        Ok(())
    }

    async fn find_by_slug(&self, slug: &str) -> R<Option<HelpArticle>> {
        Ok(self.items.lock().unwrap().get(slug).cloned())
    }

    async fn list_by_category(
        &self,
        category_id: HelpCategoryId,
        only_published: bool,
    ) -> R<Vec<HelpArticle>> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .filter(|a| {
                a.category_id == category_id
                    && (!only_published || a.status == HelpArticleStatus::Published)
            })
            .cloned()
            .collect())
    }

    async fn list_published(&self) -> R<Vec<HelpArticle>> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .filter(|a| a.status == HelpArticleStatus::Published)
            .cloned()
            .collect())
    }

    async fn search(&self, query: &str) -> R<Vec<HelpArticle>> {
        let q = query.to_lowercase();
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .filter(|a| {
                a.status == HelpArticleStatus::Published
                    && (a.title.to_lowercase().contains(&q) || a.content.to_lowercase().contains(&q))
            })
            .cloned()
            .collect())
    }
}
EOF_PINGVERTEX

mkdir -p "$(dirname "crates/infra/src/repositories/postgres_help_article_repository.rs")"
cat > "crates/infra/src/repositories/postgres_help_article_repository.rs" <<'EOF_PINGVERTEX'
use application::ports::HelpArticleRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::HelpArticle;
use domain::enums::HelpArticleStatus;
use domain::shared::ids::{HelpArticleId, HelpCategoryId};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresHelpArticleRepository {
    pool: PgPool,
}

impl PostgresHelpArticleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct HelpArticleRow {
    id: Uuid,
    category_id: Uuid,
    title: String,
    slug: String,
    content: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HelpArticleRow {
    fn into_domain(self) -> Result<HelpArticle, Box<dyn Error + Send + Sync>> {
        let status: HelpArticleStatus = self
            .status
            .parse()
            .map_err(|e: String| std::io::Error::other(e))?;
        Ok(HelpArticle {
            id: HelpArticleId(self.id),
            category_id: HelpCategoryId(self.category_id),
            title: self.title,
            slug: self.slug,
            content: self.content,
            status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[async_trait]
impl HelpArticleRepository for PostgresHelpArticleRepository {
    async fn create(&self, article: &HelpArticle) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO help_articles (id, category_id, title, slug, content, status, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(article.id.0)
        .bind(article.category_id.0)
        .bind(&article.title)
        .bind(&article.slug)
        .bind(&article.content)
        .bind(article.status.as_str())
        .bind(article.created_at)
        .bind(article.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_slug(&self, slug: &str) -> RepoResult<Option<HelpArticle>> {
        let row = sqlx::query_as::<_, HelpArticleRow>(
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        row.map(HelpArticleRow::into_domain).transpose()
    }

    async fn list_by_category(
        &self,
        category_id: HelpCategoryId,
        only_published: bool,
    ) -> RepoResult<Vec<HelpArticle>> {
        let query = if only_published {
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE category_id = $1 AND status = 'published' ORDER BY created_at DESC"
        } else {
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE category_id = $1 ORDER BY created_at DESC"
        };
        let rows = sqlx::query_as::<_, HelpArticleRow>(query)
            .bind(category_id.0)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(HelpArticleRow::into_domain).collect()
    }

    async fn list_published(&self) -> RepoResult<Vec<HelpArticle>> {
        let rows = sqlx::query_as::<_, HelpArticleRow>(
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE status = 'published' ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(HelpArticleRow::into_domain).collect()
    }

    async fn search(&self, query: &str) -> RepoResult<Vec<HelpArticle>> {
        let pattern = format!("%{}%", query);
        let rows = sqlx::query_as::<_, HelpArticleRow>(
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE status = 'published' AND (title ILIKE $1 OR content ILIKE $1) ORDER BY created_at DESC",
        )
        .bind(pattern)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(HelpArticleRow::into_domain).collect()
    }
}
EOF_PINGVERTEX

echo "PingVertex frontend coverage changes applied."
echo "Changed files written: 47"

echo ""
echo "Verification is optional but recommended. Set SKIP_VERIFY=1 to skip it."
if [[ "${SKIP_VERIFY:-0}" != "1" ]]; then
    if command -v cargo >/dev/null 2>&1; then
        cargo fmt --all
        cargo clippy --workspace --all-targets -- -D warnings
        cargo test -p leptos-dashboard
        cargo test -p api
        cargo test -p application
        cargo test -p infra
        echo "Verification commands completed."
    else
        echo "cargo is not installed; source changes were applied but Rust verification was skipped."
        echo "Install the Rust toolchain and rerun with SKIP_VERIFY=0."
    fi
fi
