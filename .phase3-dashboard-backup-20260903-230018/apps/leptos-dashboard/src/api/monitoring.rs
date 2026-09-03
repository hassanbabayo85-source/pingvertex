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
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Default,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Default,
)]
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
