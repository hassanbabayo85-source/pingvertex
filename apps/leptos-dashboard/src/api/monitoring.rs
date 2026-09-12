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
    pub method: String,
    pub url: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub enabled: bool,
    #[serde(default)]
    pub billing_blocked: bool,
    #[serde(default)]
    pub billing_block_reason: Option<String>,
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
    /// Authenticated monitor response field used to preserve assertions during edits.
    #[serde(default)]
    pub assertions: Vec<serde_json::Value>,
    #[serde(default)]
    pub regions: Vec<String>,
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
    #[serde(default)]
    pub monitor_name: String,
    pub started_at: String,
    pub acknowledged_at: Option<String>,
    pub resolved_at: Option<String>,
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
    #[serde(default)]
    pub kind: String,
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
#[serde(default)]
pub struct DashboardTrendPoint {
    pub bucket: String,
    pub uptime_percentage: Option<f64>,
    pub avg_latency_ms: Option<f64>,
    pub checks_count: u64,
    pub incident_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct DashboardPerformance {
    pub uptime_percentage: Option<f64>,
    pub avg_latency_ms: Option<f64>,
    pub trend: Vec<DashboardTrendPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct DashboardOverview {
    pub monitor_summary: MonitorSummary,
    pub monitors: Vec<MonitorOverviewItem>,
    pub incident_summary: IncidentSummaryData,
    pub performance: DashboardPerformance,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct DashboardBadges {
    pub active_incidents: u32,
    pub unread_notifications: u32,
    pub pending_invitations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct RunChecksResponse {
    pub queued: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Check {
    pub id: String,
    pub monitor_id: String,
    pub status: String,
    pub latency_ms: u128,
    pub checked_at: String,
    pub region: String,
    pub error_message: Option<String>,
    #[serde(default)]
    pub cert_expires_at: Option<String>,
    #[serde(default)]
    pub cert_days_remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Incident {
    pub id: String,
    pub monitor_id: String,
    pub first_seen: String,
    pub last_seen_at: Option<String>,
    pub resolved_at: Option<String>,
    pub duration_ms: i64,
    pub acknowledged_at: Option<String>,
    pub acknowledged_by: Option<String>,
    pub occurred_during_maintenance: bool,
    pub escalation_step_index: u32,
    pub last_escalation_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct NotificationChannel {
    pub id: String,
    pub monitor_id: String,
    pub kind: String,
    pub target: String,
    pub enabled: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct MaintenanceWindow {
    pub id: String,
    pub monitor_id: String,
    pub starts_at: String,
    pub ends_at: String,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct EscalationStep {
    pub delay_minutes: u32,
    pub channel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct EscalationPolicy {
    pub id: String,
    pub monitor_id: Option<String>,
    pub steps: Vec<EscalationStep>,
    pub created_at: String,
    pub updated_at: String,
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
    #[serde(default)]
    pub regions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelPayload {
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

#[derive(Debug, Clone, Serialize)]
pub struct BulkMonitorActionRequest {
    pub action: String,
    #[serde(default)]
    pub monitor_ids: Vec<String>,
    #[serde(default)]
    pub all: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BulkMonitorActionResponse {
    pub job_id: String,
    pub status: String,
    pub affected: u64,
    pub action: String,
}

pub async fn bulk_monitor_action(
    action: &str,
    monitor_ids: Vec<String>,
    all: bool,
) -> Result<BulkMonitorActionResponse, String> {
    client::post_json(
        "/monitors/bulk",
        &BulkMonitorActionRequest {
            action: action.to_owned(),
            monitor_ids,
            all,
            reason: None,
        },
    )
    .await
    .map_err(to_string_err)
}

pub async fn fetch_overview() -> Result<DashboardOverview, String> {
    client::get_json::<DashboardOverview>("/dashboard/overview")
        .await
        .map_err(to_string_err)
}

pub async fn fetch_badges() -> Result<DashboardBadges, String> {
    client::get_json::<DashboardBadges>("/dashboard/badges")
        .await
        .map_err(to_string_err)
}

pub async fn run_checks_now() -> Result<RunChecksResponse, String> {
    client::post_json::<RunChecksResponse, _>("/dashboard/checks/now", &serde_json::json!({}))
        .await
        .map_err(to_string_err)
}

pub async fn fetch_incident(incident_id: &str) -> Result<Incident, String> {
    client::get_json::<Incident>(&format!("/incidents/{incident_id}"))
        .await
        .map_err(to_string_err)
}

pub async fn fetch_incidents(monitor_id: &str) -> Result<Vec<Incident>, String> {
    client::get_json::<Vec<Incident>>(&format!("/monitors/{monitor_id}/incidents"))
        .await
        .map_err(to_string_err)
}

pub async fn fetch_checks(monitor_id: &str) -> Result<Vec<Check>, String> {
    client::get_json::<Vec<Check>>(&format!("/monitors/{monitor_id}/checks?limit=50&offset=0"))
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

pub async fn list_maintenance_windows(monitor_id: &str) -> Result<Vec<MaintenanceWindow>, String> {
    client::get_json::<Vec<MaintenanceWindow>>(&format!(
        "/monitors/{monitor_id}/maintenance-windows"
    ))
    .await
    .map_err(to_string_err)
}

pub async fn list_escalation_policies() -> Result<Vec<EscalationPolicy>, String> {
    client::get_json::<Vec<EscalationPolicy>>("/escalation-policies")
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

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct OneTimeCheckResponse {
    pub id: String,
    pub request_id: String,
    pub status: String,
}
pub async fn run_one_time_check(id: &str) -> Result<OneTimeCheckResponse, String> {
    client::post_json(
        &format!("/monitors/{id}/checks/one-time"),
        &serde_json::json!({}),
    )
    .await
    .map_err(|e| e.to_string())
}
pub async fn get_one_time_check(
    monitor_id: &str,
    request_id: &str,
) -> Result<OneTimeCheckResponse, String> {
    client::get_json(&format!(
        "/monitors/{monitor_id}/checks/one-time/{request_id}"
    ))
    .await
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BulkImportError {
    pub row: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BulkImportPricingVersion {
    pub operation: String,
    pub account_type: Option<String>,
    pub monitor_kind: String,
    pub region: String,
    pub quantity: u64,
    pub unit_price: i64,
    pub estimated_charge_units: i64,
    pub currency: String,
    pub version: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BulkImportPreview {
    pub total_rows: usize,
    pub valid_rows: usize,
    pub invalid_rows: usize,
    pub duplicate_rows: usize,
    pub forecast_days: u32,
    pub estimated_execution_quantity: u64,
    pub estimated_payg_cost_units: i64,
    pub currency: String,
    pub pricing_version: Option<i64>,
    pub pricing_versions: Vec<BulkImportPricingVersion>,
    pub pricing_snapshot_hash: String,
    pub payg_eligible: bool,
    pub errors: Vec<BulkImportError>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BulkImportJob {
    pub id: String,
    pub status: String,
    pub total_rows: i32,
    pub valid_rows: i32,
    pub invalid_rows: i32,
    pub processed_rows: i32,
    pub created_rows: i32,
    pub failed_rows: i32,
    pub skipped_rows: i32,
    /// Server-computed forecast summary, carried through to the final
    /// import summary so the user can see what the run was estimated to
    /// cost without opening the preview again.
    pub forecast_days: i32,
    pub estimated_checks: i64,
    pub estimated_charge_units: i64,
    pub currency: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BulkImportConfirmResponse {
    pub job: BulkImportJob,
    pub replayed: bool,
}

pub async fn bulk_import_preview(
    bytes: Vec<u8>,
    file_name: String,
    forecast_days: u32,
) -> Result<BulkImportPreview, String> {
    let path = format!("/monitors/bulk-import/preview?forecast_days={forecast_days}");
    client::post_multipart_file(&path, "file", &file_name, "text/csv", bytes)
        .await
        .map_err(|error| error.to_string())
}

pub async fn bulk_import_confirm(
    bytes: Vec<u8>,
    file_name: String,
    forecast_days: u32,
    idempotency_key: String,
    pricing_snapshot_hash: String,
) -> Result<BulkImportConfirmResponse, String> {
    let form = web_sys::FormData::new().map_err(|_| "Could not create upload form.".to_owned())?;
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let parts = js_sys::Array::new();
    parts.push(&array);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("text/csv");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|_| "Could not create upload payload.".to_owned())?;
    form.append_with_blob_and_filename("file", &blob, &file_name)
        .map_err(|_| "Could not attach upload payload.".to_owned())?;
    let url = format!("/api/v1/monitors/bulk-import/confirm?forecast_days={forecast_days}");
    let response = gloo_net::http::Request::post(&url)
        .header("Idempotency-Key", &idempotency_key)
        .header("X-Bulk-Pricing-Snapshot", &pricing_snapshot_hash)
        .credentials(web_sys::RequestCredentials::Include)
        .body(form)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !(200..300).contains(&response.status()) {
        return Err(format!("Import failed ({})", response.status()));
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn bulk_import_job(id: &str) -> Result<BulkImportJob, String> {
    client::get_json(&format!("/monitors/bulk-import/jobs/{id}"))
        .await
        .map_err(|error| error.to_string())
}

pub async fn bulk_import_pause(id: &str) -> Result<BulkImportJob, String> {
    client::post_json(
        &format!("/monitors/bulk-import/jobs/{id}/pause"),
        &serde_json::json!({}),
    )
    .await
    .map_err(|error| error.to_string())
}

pub async fn bulk_import_resume(id: &str) -> Result<BulkImportJob, String> {
    client::post_json(
        &format!("/monitors/bulk-import/jobs/{id}/resume"),
        &serde_json::json!({}),
    )
    .await
    .map_err(|error| error.to_string())
}

pub async fn fetch_bulk_monitor_action_job(
    job_id: &str,
) -> Result<BulkMonitorActionResponse, String> {
    client::get_json::<BulkMonitorActionResponse>(&format!("/monitors/bulk/jobs/{job_id}"))
        .await
        .map_err(to_string_err)
}
