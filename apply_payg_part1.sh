#!/usr/bin/env bash
set -euo pipefail

PROJECT="${PROJECT:-$HOME/uptime-monitor}"
cd "$PROJECT"
test -f Cargo.toml || { echo "ERROR: $PROJECT is not an uptime-monitor workspace"; exit 1; }

echo "Applying PingVertex PAYG Blueprint Part 1 changes..."

mkdir -p "$(dirname "$PROJECT/apps/leptos-dashboard/src/api/billing.rs")"
cat <<'EOF' > "$PROJECT/apps/leptos-dashboard/src/api/billing.rs"
use crate::api::client::{self, ApiError};
use js_sys::encode_uri_component;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AddOn {
    pub id: String,
    pub addon_type: String,
    pub quantity: u64,
    pub tier: Option<String>,
    pub unit_price: String,
    pub total_price: String,
    pub currency: String,
    pub status: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Subscription {
    pub id: String,
    pub tenant_id: String,
    pub plan_tier: String,
    pub status: String,
    pub trial_ends_at: Option<String>,
    pub grace_period_ends_at: Option<String>,
    pub current_period_ends_at: Option<String>,
    pub canceled_at: Option<String>,
    pub cancel_at_period_end: bool,
    pub monitors: Option<u64>,
    pub interval_seconds: Option<u32>,
    pub retention_days: Option<u32>,
    pub pricing_version: Option<u32>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Plan {
    pub tier: String,
    pub max_monitors: u32,
    pub min_check_interval_seconds: u32,
    pub history_retention_days: u32,
    pub max_locations: u32,
    pub max_team_members: u32,
    pub rum_enabled: bool,
    pub advanced_features: bool,
    pub monthly_price: String,
    pub currency: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BillingSummary {
    pub base_price: String,
    pub add_ons: Vec<AddOn>,
    pub add_ons_total: String,
    pub total: String,
    pub currency: String,
    pub pricing_version: u32,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Quote {
    pub is_free: bool,
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
    pub interval_label: String,
    pub retention_label: String,
    pub checks_per_period: u64,
    pub compute: CostLine,
    pub network: CostLine,
    pub storage: StorageCost,
    pub backup: CostLine,
    pub infrastructure_cost: String,
    pub markup_amount: String,
    pub monthly_price: String,
    pub currency: String,
    pub pricing_version: u32,
    pub generated_at: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CostLine {
    pub base: String,
    pub buffer: String,
    pub adjusted: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StorageCost {
    pub base: String,
    pub buffer: String,
    pub adjusted: String,
    pub storage_breakdown: Vec<StorageTierCost>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct StorageTierCost {
    pub tier: String,
    pub range_days: String,
    pub volume_gb: String,
    pub base_cost: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct QuoteRequest {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutRequest {
    pub plan_tier: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CheckoutResponse {
    pub checkout_session_id: String,
    pub checkout_url: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct VerifyCheckoutRequest {
    pub checkout_session_id: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct VerifyCheckoutResponse {
    pub status: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct AddOnRequest {
    pub addon_type: String,
    pub quantity: u64,
    pub country_code: Option<String>,
    pub tier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaygPricing { pub operation:String, pub unit_name:String, pub unit_price:i64, pub currency:String, pub version:i64, pub effective_from:String }
pub async fn payg_pricing() -> Result<Vec<PaygPricing>, ApiError> { client::get_json("/payg/pricing").await }

pub async fn subscription() -> Result<Option<Subscription>, ApiError> {
    client::get_json("/billing/subscription").await
}
pub async fn plans() -> Result<Vec<Plan>, ApiError> {
    client::get_json("/billing/plans").await
}
pub async fn cancel_subscription(at_period_end: bool) -> Result<Subscription, ApiError> {
    client::post_json(
        "/billing/subscription/cancel",
        &serde_json::json!({"at_period_end":at_period_end}),
    )
    .await
}
pub async fn summary() -> Result<BillingSummary, ApiError> {
    client::get_json("/billing/summary").await
}
pub async fn quote(req: &QuoteRequest) -> Result<Quote, ApiError> {
    let path = format!(
        "/billing/quote?monitors={}&interval_seconds={}&retention_days={}",
        encode_uri_component(&req.monitors.to_string())
            .as_string()
            .unwrap_or_default(),
        encode_uri_component(&req.interval_seconds.to_string())
            .as_string()
            .unwrap_or_default(),
        encode_uri_component(&req.retention_days.to_string())
            .as_string()
            .unwrap_or_default(),
    );
    client::get_json(&path).await
}
pub async fn checkout(plan_tier: &str) -> Result<CheckoutResponse, ApiError> {
    client::post_json(
        "/billing/checkout",
        &CheckoutRequest {
            plan_tier: plan_tier.to_string(),
        },
    )
    .await
}
pub async fn verify_checkout(id: &str) -> Result<VerifyCheckoutResponse, ApiError> {
    client::post_json(
        "/billing/checkout/verify",
        &VerifyCheckoutRequest {
            checkout_session_id: id.to_string(),
        },
    )
    .await
}
pub async fn add_on(req: &AddOnRequest) -> Result<AddOn, ApiError> {
    client::post_json("/billing/addons", req).await
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Invoice {
    pub invoice_number: String,
    pub currency: String,
    pub subtotal: String,
    pub tax: String,
    pub total: String,
    pub status: String,
    pub period_start: String,
    pub period_end: String,
    pub created_at: String,
    pub paid_at: Option<String>,
}
pub async fn invoices() -> Result<Vec<Invoice>, ApiError> {
    client::get_json("/billing/invoices").await
}
EOF

mkdir -p "$(dirname "$PROJECT/apps/leptos-dashboard/src/api/monitoring.rs")"
cat <<'EOF' > "$PROJECT/apps/leptos-dashboard/src/api/monitoring.rs"
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
pub struct OneTimeCheckResponse { pub id:String, pub request_id:String, pub status:String }
pub async fn run_one_time_check(id:&str)->Result<OneTimeCheckResponse,String>{ client::post_json(&format!("/monitors/{id}/checks/one-time"), &serde_json::json!({})).await.map_err(|e|e.to_string()) }
pub async fn get_one_time_check(monitor_id:&str,request_id:&str)->Result<OneTimeCheckResponse,String>{ client::get_json(&format!("/monitors/{monitor_id}/checks/one-time/{request_id}")).await.map_err(|e|e.to_string()) }
EOF

mkdir -p "$(dirname "$PROJECT/apps/leptos-dashboard/src/api/wallet.rs")"
cat <<'EOF' > "$PROJECT/apps/leptos-dashboard/src/api/wallet.rs"
use crate::api::client::{self, ApiError};
use js_sys::encode_uri_component;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Wallet {
    pub wallet_id: String,
    pub currency: String,
    pub balance_units: i64,
    pub reserved_units: i64,
    pub available_units: i64,
    pub status: String,
    pub low_balance_threshold_units: i64,
    pub critical_balance_threshold_units: i64,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WalletNotification {
    pub id: String,
    pub event_type: String,
    pub message: String,
    pub read_at: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RefundRequest {
    pub provider_payment_id: String,
    pub amount_units: i64,
    pub currency: Option<String>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RefundResponse {
    pub provider_refund_id: String,
    pub status: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WalletTransaction {
    pub id: String,
    pub transaction_type: String,
    pub amount_units: i64,
    pub balance_after_units: i64,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct TopUpRequest {
    pub amount_units: i64,
    pub payment_method: Option<String>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TopUpResponse {
    pub checkout_url: String,
    pub provider_checkout_id: String,
    pub amount_units: i64,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WalletSettings {
    pub tenant_id: String,
    pub low_balance_threshold_units: i64,
    pub critical_balance_threshold_units: i64,
    pub auto_top_up_enabled: bool,
    pub auto_top_up_threshold_units: i64,
    pub auto_top_up_amount_units: i64,
    pub payment_method: Option<String>,
    pub daily_top_up_limit_units: i64,
    pub monthly_top_up_limit_units: i64,
    pub retry_limit: i32,
}
#[derive(Debug, Clone, Serialize)]
pub struct WalletSettingsRequest {
    pub low_balance_threshold_units: i64,
    pub critical_balance_threshold_units: i64,
    pub auto_top_up_enabled: bool,
    pub auto_top_up_threshold_units: i64,
    pub auto_top_up_amount_units: i64,
    pub payment_method: Option<String>,
    pub daily_top_up_limit_units: i64,
    pub monthly_top_up_limit_units: i64,
    pub retry_limit: i32,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UsageReport {
    pub current_balance_units: i64,
    pub today_usage_units: i64,
    pub month_usage_units: i64,
    pub month_spend_units: i64,
    pub projected_spend_units: i64,
    pub daily: Vec<DailyPoint>,
    pub by_operation: Vec<OperationPoint>,
    pub rows: Vec<UsageRow>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DailyPoint {
    pub day: String,
    pub usage_units: i64,
    pub spend_units: i64,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct OperationPoint {
    pub operation: String,
    pub usage_units: i64,
    pub spend_units: i64,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UsageRow {
    pub timestamp: String,
    pub operation: String,
    pub units: i64,
    pub price_units: i64,
    pub charge_units: i64,
    pub request_id: String,
    pub status: String,
}

pub async fn get() -> Result<Wallet, ApiError> {
    client::get_json("/wallet").await
}
pub async fn transactions() -> Result<Vec<WalletTransaction>, ApiError> {
    client::get_json("/wallet/transactions").await
}
pub async fn notifications() -> Result<Vec<WalletNotification>, ApiError> {
    client::get_json("/wallet/notifications").await
}
pub async fn refund(req: &RefundRequest) -> Result<RefundResponse, ApiError> {
    client::post_json("/wallet/refund", req).await
}
pub async fn top_up(req: &TopUpRequest) -> Result<TopUpResponse, ApiError> {
    client::post_json("/wallet/top-up", req).await
}
pub async fn settings() -> Result<WalletSettings, ApiError> {
    client::get_json("/wallet/settings").await
}
pub async fn update_settings(req: &WalletSettingsRequest) -> Result<WalletSettings, ApiError> {
    client::put_json("/wallet/settings", req).await
}
pub async fn usage(
    from: Option<&str>,
    to: Option<&str>,
    operation: Option<&str>,
    status: Option<&str>,
) -> Result<UsageReport, ApiError> {
    let mut query = Vec::new();
    if let Some(v) = from {
        if !v.is_empty() {
            query.push(format!(
                "from={}",
                encode_uri_component(v).as_string().unwrap_or_default()
            ));
        }
    }
    if let Some(v) = to {
        if !v.is_empty() {
            query.push(format!(
                "to={}",
                encode_uri_component(v).as_string().unwrap_or_default()
            ));
        }
    }
    if let Some(v) = operation {
        if !v.is_empty() {
            query.push(format!(
                "operation={}",
                encode_uri_component(v).as_string().unwrap_or_default()
            ));
        }
    }
    if let Some(v) = status {
        if !v.is_empty() {
            query.push(format!(
                "status={}",
                encode_uri_component(v).as_string().unwrap_or_default()
            ));
        }
    }
    let path = if query.is_empty() {
        "/wallet/usage".to_owned()
    } else {
        format!("/wallet/usage?{}", query.join("&"))
    };
    client::get_json(&path).await
}
EOF

mkdir -p "$(dirname "$PROJECT/apps/leptos-dashboard/src/components/navigation.rs")"
cat <<'EOF' > "$PROJECT/apps/leptos-dashboard/src/components/navigation.rs"
//! Navigation model for the authenticated app shell.
//!
//! This module is intentionally data-only: it describes which product areas
//! exist, which route each one points at, and whether that route is
//! registered yet. `components::layout::Layout` renders this data. Keeping
//! the data separate from the render code means Path 10 (or any future
//! integration pass) can add a future navigation item in one place, in
//! `routes/mod.rs`-adjacent order.
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
        label: "TOOLS",
        items: &[NavItem {
            label: "Proxy Checker",
            href: "/proxy-check",
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
                href: "/incidents",
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
                label: "PAYG billing",
                href: "/billing",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Wallet",
                href: "/billing/wallet",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Usage",
                href: "/billing/usage",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Invoices",
                href: "/billing/invoices",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Transactions",
                href: "/billing/transactions",
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
EOF

mkdir -p "$(dirname "$PROJECT/apps/leptos-dashboard/src/pages/billing.rs")"
cat <<'EOF' > "$PROJECT/apps/leptos-dashboard/src/pages/billing.rs"
use crate::api::{billing,wallet}; use leptos::prelude::*; use leptos::task::spawn_local;
fn usd(u:i64)->String{format!("${}.{:06}",u/1_000_000,u.abs()%1_000_000)}
#[component] pub fn BillingPage()->impl IntoView{let w=RwSignal::new(None::<wallet::Wallet>);let p=RwSignal::new(Vec::<billing::PaygPricing>::new());let e=RwSignal::new(None::<String>);Effect::new(move |_|{spawn_local(async move{match wallet::get().await{Ok(v)=>w.set(Some(v)),Err(x)=>e.set(Some(x.to_string()))};match billing::payg_pricing().await{Ok(v)=>p.set(v),Err(x)=>e.set(Some(x.to_string()))}})});view!{<div class="feature-page billing-page"><header class="page-header"><div><span class="eyebrow">"PAYG"</span><h1>"Billing"</h1><p class="page-subtitle">"Prepaid usage. No subscription plans."</p></div></header>{move||e.get().map(|x|view!{<div class="error" role="alert">{x}</div>})}{move||w.get().map(|x|view!{<section class="card"><div class="section-heading"><div><h2>"Wallet"</h2><p class="muted">"Monitoring and billable operations consume wallet credit."</p></div><a class="btn btn-primary" href="/billing/wallet">"Add funds"</a></div><div class="overview-cards"><div><span class="card-label">"Balance"</span><span class="card-value">{usd(x.balance_units)}</span></div><div><span class="card-label">"Available"</span><span class="card-value">{usd(x.available_units)}</span></div><div><span class="card-label">"Reserved"</span><span class="card-value">{usd(x.reserved_units)}</span></div><div><span class="card-label">"Status"</span><span class="card-value">{x.status}</span></div><div><span class="card-label">"Minimum top-up"</span><span class="card-value">"$5"</span></div></div></section>})}<section class="card"><h2>"Price book"</h2><p class="muted">"Prices are versioned and resolved by the server; clients cannot submit prices."</p><div class="overview-cards">{move||p.get().into_iter().map(|x|view!{<div><span class="card-label">{x.operation}</span><span class="card-value">{usd(x.unit_price)}</span><span class="muted">{format!("per {} · v{}",x.unit_name,x.version)}</span></div>}).collect_view()}</div></section><section class="card"><h2>"Free entitlement"</h2><p class="muted">"Up to 10 monitors at 5-minute intervals. More monitors or faster intervals require a funded wallet."</p></section></div>}}
EOF

mkdir -p "$(dirname "$PROJECT/apps/leptos-dashboard/src/pages/dashboard_overview.rs")"
cat <<'EOF' > "$PROJECT/apps/leptos-dashboard/src/pages/dashboard_overview.rs"
//! Flagship dashboard overview backed by tenant-scoped aggregate APIs.

use crate::api::monitoring::{self, DashboardOverview, DashboardTrendPoint, MonitorOverviewItem};
use crate::auth::context::{use_auth, AuthState};
use crate::components::error_state::ErrorState;
use crate::components::loading::LoadingSpinner;
use crate::components::polling;
use crate::components::status_badge::StatusBadge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

const RECENT_MONITORS_LIMIT: usize = 8;
const TREND_HEIGHT: f64 = 120.0;
const TREND_BOTTOM: f64 = 116.0;

fn user_facing_error(message: &str) -> String {
    if message.starts_with("Network error:") {
        "Unable to reach PingVertex. Check your connection and try again.".to_string()
    } else if message.starts_with("Server error") {
        "PingVertex could not load the dashboard right now. Please try again.".to_string()
    } else {
        message.to_string()
    }
}

fn format_number(value: f64) -> String {
    if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn format_timestamp(value: &str) -> String {
    value
        .strip_suffix('Z')
        .unwrap_or(value)
        .replace('T', " ")
        .split('.')
        .next()
        .unwrap_or(value)
        .to_string()
}

fn hour_label(value: &str) -> String {
    let hour = value
        .split('T')
        .nth(1)
        .and_then(|part| part.get(0..2))
        .and_then(|h| h.parse::<u32>().ok())
        .unwrap_or(0);
    let suffix = if hour >= 12 { "PM" } else { "AM" };
    let display = match hour % 12 {
        0 => 12,
        value => value,
    };
    format!("{display}{suffix}")
}

fn relative_time(value: Option<&str>, now_ms: f64) -> String {
    let Some(value) = value else {
        return "No check yet".to_string();
    };
    let parsed = js_sys::Date::parse(value);
    if !parsed.is_finite() {
        return format_timestamp(value);
    }
    let seconds = ((now_ms - parsed) / 1000.0).round() as i64;
    if seconds < 0 {
        return "in a moment".to_string();
    }
    if seconds < 60 {
        return format!("{seconds}s ago");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    format!("{}d ago", hours / 24)
}

fn delta_percent(first: Option<f64>, last: Option<f64>) -> Option<f64> {
    let (first, last) = (first?, last?);
    if first.abs() < f64::EPSILON {
        return None;
    }
    Some(((last - first) / first.abs()) * 100.0)
}

fn trend_delta(trend: &[DashboardTrendPoint], metric: &str) -> Option<f64> {
    let values: Vec<f64> = trend
        .iter()
        .filter_map(|point| match metric {
            "uptime" => point.uptime_percentage,
            "latency" => point.avg_latency_ms,
            _ => Some(point.incident_count as f64),
        })
        .collect();
    delta_percent(values.first().copied(), values.last().copied())
}

fn monitor_type(kind: &str, url: &str) -> (&'static str, &'static str) {
    match kind.to_ascii_lowercase().as_str() {
        "tcp" => ("tcp", "TCP"),
        "dns" => ("dns", "DNS"),
        "heartbeat" => ("heartbeat", "Heartbeat"),
        "flow" => ("flow", "API flow"),
        "http" if url.starts_with("https://") => ("https", "HTTPS"),
        _ => ("http", "HTTP"),
    }
}

fn monitor_status(monitor: &MonitorOverviewItem) -> String {
    if !monitor.enabled {
        "disabled".to_string()
    } else {
        monitor
            .status
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
            .to_ascii_lowercase()
    }
}

struct TrendPoint {
    x: f64,
    y: f64,
}

struct TrendSeries {
    line_paths: Vec<String>,
    area_paths: Vec<String>,
    markers: Vec<(TrendPoint, String, f64)>,
}

fn smooth_path(points: &[TrendPoint]) -> String {
    if points.len() < 2 {
        return String::new();
    }

    let mut path = format!("M {:.1},{:.1}", points[0].x, points[0].y);
    for i in 0..points.len() - 1 {
        let p0 = if i == 0 { &points[i] } else { &points[i - 1] };
        let p1 = &points[i];
        let p2 = &points[i + 1];
        let p3 = if i + 2 < points.len() {
            &points[i + 2]
        } else {
            p2
        };
        let c1x = p1.x + (p2.x - p0.x) / 6.0;
        let c1y = p1.y + (p2.y - p0.y) / 6.0;
        let c2x = p2.x - (p3.x - p1.x) / 6.0;
        let c2y = p2.y - (p3.y - p1.y) / 6.0;
        path.push_str(&format!(
            " C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
            c1x, c1y, c2x, c2y, p2.x, p2.y
        ));
    }
    path
}

fn trend_series(
    trend: &[DashboardTrendPoint],
    value: impl Fn(&DashboardTrendPoint) -> Option<f64>,
) -> Option<TrendSeries> {
    if trend.len() < 2 {
        return None;
    }

    let present: Vec<(usize, f64)> = trend
        .iter()
        .enumerate()
        .filter_map(|(index, point)| value(point).map(|v| (index, v)))
        .collect();
    if present.len() < 2 {
        return None;
    }

    let min = present
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::INFINITY, f64::min);
    let max = present
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::NEG_INFINITY, f64::max);
    let range = (max - min).max(0.000_001);
    let width = 720.0;
    let step = width / (trend.len().saturating_sub(1).max(1) as f64);

    let point_for = |index: usize, v: f64| TrendPoint {
        x: index as f64 * step,
        y: 8.0 + ((max - v) / range) * (TREND_HEIGHT - 16.0),
    };

    let mut line_paths = Vec::new();
    let mut area_paths = Vec::new();
    let mut markers = Vec::new();
    let mut current: Vec<TrendPoint> = Vec::new();
    let mut start_index = 0usize;
    let mut previous_index: Option<usize> = None;

    for (index, value) in present {
        if previous_index
            .map(|previous| index != previous + 1)
            .unwrap_or(false)
        {
            if current.len() >= 2 {
                let line = smooth_path(&current);
                let area = format!(
                    "{line} L {:.1},{TREND_BOTTOM:.1} L {:.1},{TREND_BOTTOM:.1} Z",
                    current.last().unwrap().x,
                    current.first().unwrap().x
                );
                line_paths.push(line);
                area_paths.push(area);
            }
            current.clear();
            start_index = index;
        }

        let point = point_for(index, value);
        markers.push((point_for(index, value), trend[index].bucket.clone(), value));
        current.push(point);
        previous_index = Some(index);
    }

    let _ = start_index;
    if current.len() >= 2 {
        let line = smooth_path(&current);
        let area = format!(
            "{line} L {:.1},{TREND_BOTTOM:.1} L {:.1},{TREND_BOTTOM:.1} Z",
            current.last().unwrap().x,
            current.first().unwrap().x
        );
        line_paths.push(line);
        area_paths.push(area);
    }

    Some(TrendSeries {
        line_paths,
        area_paths,
        markers,
    })
}

#[component]
fn TrendIndicator(
    #[prop(optional)] delta: Option<f64>,
    #[prop(optional)] inverse: bool,
) -> impl IntoView {
    let Some(delta) = delta else {
        return view! { <span class="trend-indicator trend-neutral">"—"</span> }.into_any();
    };
    let positive = if inverse { delta < 0.0 } else { delta > 0.0 };
    let arrow = if delta.abs() < 0.05 {
        "→"
    } else if positive {
        "↑"
    } else {
        "↓"
    };
    let class = if delta.abs() < 0.05 {
        "trend-indicator trend-neutral"
    } else if positive {
        "trend-indicator trend-positive"
    } else {
        "trend-indicator trend-negative"
    };
    view! { <span class=class>{format!("{arrow} {:.1}%", delta.abs())}</span> }.into_any()
}

#[component]
fn TrendChart(
    #[prop(into)] title: String,
    #[prop(into)] value: String,
    #[prop(into)] description: String,
    trend: Vec<DashboardTrendPoint>,
    #[prop(into)] metric: String,
    #[prop(into)] unit: String,
) -> impl IntoView {
    let series = match metric.as_str() {
        "uptime" => trend_series(&trend, |point| point.uptime_percentage),
        "latency" => trend_series(&trend, |point| point.avg_latency_ms),
        _ => trend_series(&trend, |point| Some(point.incident_count as f64)),
    };
    let delta = trend_delta(&trend, &metric);
    let inverse = metric == "latency";
    let gradient_id = format!("trend-fill-{}", metric);

    let format_marker_value = |v: f64| match metric.as_str() {
        "uptime" => format!("{v:.2}%"),
        "latency" => format!("{v:.0} ms"),
        _ => format!("{v:.0} incidents"),
    };

    view! {
        <section class="dashboard-widget dashboard-trend-card" aria-label=format!("{}: last 24 hours", title.clone())>
            <div class="widget-heading">
                <div>
                    <div class="trend-title-row">
                        <h2>{title.clone()}</h2>
                        <TrendIndicator delta=delta.unwrap_or(0.0) inverse=inverse/>
                    </div>
                    <p class="widget-description">{description}</p>
                </div>
                <strong class="trend-value">{value} {unit}</strong>
            </div>
            {if let Some(series) = series {
                let area_views = series.area_paths.iter().map(|path| view! {
                    <path class="trend-area" d=path.clone() fill=format!("url(#{gradient_id})")/>
                }).collect_view();
                let line_views = series.line_paths.iter().map(|path| view! {
                    <path class="trend-line" d=path.clone() fill="none"/>
                }).collect_view();
                let marker_views = series.markers.iter().map(|(point, bucket, value)| {
                    let label = format!("{}: {}", format_timestamp(bucket), format_marker_value(*value));
                    view! {
                        <circle class="trend-point" cx=format!("{:.1}", point.x) cy=format!("{:.1}", point.y) r="3.5" tabindex="0" role="img" aria-label=label.clone()>
                            <title>{label.clone()}</title>
                        </circle>
                    }
                }).collect_view();
                let axis_labels = trend.iter().enumerate().filter_map(|(index, point)| {
                    if index % 4 == 0 { Some(hour_label(&point.bucket)) } else { None }
                }).collect::<Vec<_>>();
                view! {
                    <div class="trend-chart-wrap">
                        <svg class="trend-chart" viewBox="0 0 720 120" preserveAspectRatio="none" role="img" aria-label="24 hour performance trend">
                            <defs>
                                <linearGradient id=gradient_id x1="0" y1="0" x2="0" y2="1">
                                    <stop offset="0%" stop-color="currentColor" stop-opacity="0.22"/>
                                    <stop offset="100%" stop-color="currentColor" stop-opacity="0"/>
                                </linearGradient>
                            </defs>
                            <line class="trend-grid" x1="0" y1="20" x2="720" y2="20"/>
                            <line class="trend-grid" x1="0" y1="60" x2="720" y2="60"/>
                            <line class="trend-grid" x1="0" y1="100" x2="720" y2="100"/>
                            {area_views}
                            {line_views}
                            {marker_views}
                        </svg>
                    </div>
                    <div class="trend-axis" aria-hidden="true">
                        {axis_labels.into_iter().map(|label| view! { <span>{label}</span> }).collect_view()}
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="trend-empty">
                        <span>"Not enough telemetry yet."</span>
                        <small>"A trend appears after at least two data points are available."</small>
                    </div>
                }.into_any()
            }}
        </section>
    }
}

#[component]
fn StatSparkline(#[prop(optional)] values: Vec<f64>) -> impl IntoView {
    let width = 120.0;
    let height = 28.0;
    if values.len() < 2 {
        return view! { <div class="stat-sparkline stat-sparkline-empty" aria-hidden="true"></div> }.into_any();
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = (max - min).max(0.000_001);
    let step = width / (values.len() - 1) as f64;
    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = index as f64 * step;
            let y = 4.0 + ((max - value) / range) * 20.0;
            TrendPoint { x, y }
        })
        .collect::<Vec<_>>();
    let line = smooth_path(&points);
    let area = format!("{line} L {width:.1},{height:.1} L 0,{height:.1} Z");
    view! {
        <svg class="stat-sparkline" viewBox="0 0 120 28" preserveAspectRatio="none" aria-hidden="true">
            <path class="spark-area" d=area/>
            <path class="spark-line" d=line fill="none"/>
        </svg>
    }.into_any()
}

#[component]
pub fn DashboardOverviewPage() -> impl IntoView {
    let auth = use_auth();
    let overview = RwSignal::new(None::<DashboardOverview>);
    let loading = RwSignal::new(true);
    let refreshing = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let last_updated = RwSignal::new(None::<String>);
    let in_flight = RwSignal::new(false);
    let relative_now = RwSignal::new(js_sys::Date::new_0().get_time());
    let monitor_filter = RwSignal::new(String::from("all"));
    let monitor_search = RwSignal::new(String::new());
    let manual_check_busy = RwSignal::new(false);
    let manual_check_notice = RwSignal::new(None::<String>);

    let poll_data = move || {
        let overview = overview;
        let loading = loading;
        let refreshing = refreshing;
        let error = error;
        let last_updated = last_updated;
        let in_flight = in_flight;
        async move {
            if in_flight.get_untracked() {
                return Ok(());
            }
            in_flight.set(true);
            refreshing.set(true);
            match monitoring::fetch_overview().await {
                Ok(data) => {
                    overview.set(Some(data));
                    last_updated.set(Some(
                        js_sys::Date::new_0()
                            .to_iso_string()
                            .as_string()
                            .unwrap_or_else(|| "just now".to_string()),
                    ));
                    error.set(None);
                    loading.set(false);
                    refreshing.set(false);
                    in_flight.set(false);
                    Ok(())
                }
                Err(e) => {
                    error.set(Some(user_facing_error(&e)));
                    loading.set(false);
                    refreshing.set(false);
                    in_flight.set(false);
                    Err(())
                }
            }
        }
    };

    spawn_local(async move {
        let _ = poll_data().await;
    });
    polling::start(poll_data);

    let relative_timer_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let relative_timer_cleanup = relative_timer_cancel.clone();
    on_cleanup(move || relative_timer_cleanup.store(true, std::sync::atomic::Ordering::Relaxed));
    spawn_local(async move {
        use gloo_timers::future::TimeoutFuture;
        loop {
            TimeoutFuture::new(1_000).await;
            if relative_timer_cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            relative_now.set(js_sys::Date::new_0().get_time());
        }
    });

    let on_retry = Callback::new(move |_: ()| {
        spawn_local(async move {
            if in_flight.get_untracked() {
                return;
            }
            in_flight.set(true);
            refreshing.set(true);
            error.set(None);
            match monitoring::fetch_overview().await {
                Ok(data) => {
                    overview.set(Some(data));
                    last_updated.set(Some(
                        js_sys::Date::new_0()
                            .to_iso_string()
                            .as_string()
                            .unwrap_or_else(|| "just now".to_string()),
                    ));
                    loading.set(false);
                }
                Err(e) => error.set(Some(user_facing_error(&e))),
            }
            refreshing.set(false);
            loading.set(false);
            in_flight.set(false);
        });
    });

    let refresh_click = move |_| on_retry.run(());
    let run_check = move |_| {
        if manual_check_busy.get_untracked() {
            return;
        }
        manual_check_busy.set(true);
        manual_check_notice.set(None);
        spawn_local(async move {
            match monitoring::run_checks_now().await {
                Ok(result) => manual_check_notice.set(Some(format!(
                    "{} monitor{} queued for an immediate check.",
                    result.queued,
                    if result.queued == 1 { "" } else { "s" }
                ))),
                Err(e) => manual_check_notice.set(Some(user_facing_error(&e.to_string()))),
            }
            manual_check_busy.set(false);
            on_retry.run(());
        });
    };

    view! {
        <div class="dashboard-overview">
            <header class="dashboard-page-header">
                <div>
                    <p class="eyebrow">"SYSTEM OVERVIEW"</p>
                    <h1>
                        {move || {
                            let name = match auth.state.get() {
                                AuthState::Authenticated(user) => user.email.split('@').next().unwrap_or("there").to_string(),
                                _ => "there".to_string(),
                            };
                            let hour = js_sys::Date::new_0().get_hours();
                            let greeting = if hour < 12 { "Good morning" } else if hour < 18 { "Good afternoon" } else { "Good evening" };
                            format!("{greeting}, {name} 👋")
                        }}
                    </h1>
                    <p class="page-subtitle">"Here's what's happening with your services today."</p>
                    <p class="dashboard-last-updated">
                        {move || match last_updated.get() {
                            Some(value) => format!("Updated {}", format_timestamp(&value)),
                            None => "Waiting for live telemetry".to_string(),
                        }}
                    </p>
                </div>
                <div class="page-header-actions dashboard-actions">
                    <A href="/monitors/new" attr:class="btn btn-primary">"+ Add monitor"</A>
                    <A href="/status-pages" attr:class="btn">"Create status page"</A>
                    <button class="btn" type="button" on:click=run_check disabled=move || manual_check_busy.get()>
                        <span aria-hidden="true">"⚡"</span>
                        {move || if manual_check_busy.get() { "Queueing…" } else { "Run scheduled checks" }}
                    </button>
                    <button class="btn btn-ghost" type="button" on:click=refresh_click disabled=move || refreshing.get()>
                        {move || if refreshing.get() { "Refreshing…" } else { "Refresh" }}
                    </button>
                </div>
            </header>

            {move || manual_check_notice.get().map(|notice| view! { <div class="dashboard-action-notice" role="status">{notice}</div> })}

            {move || {
                if loading.get() && overview.get().is_none() {
                    view! { <LoadingSpinner text="Loading live dashboard data..."/> }.into_any()
                } else if error.get().is_some() && overview.get().is_none() {
                    let err = error.get().unwrap_or_else(|| "Unable to load dashboard.".to_string());
                    view! { <ErrorState title="Failed to load dashboard" description=err on_retry=on_retry/> }.into_any()
                } else if let Some(data) = overview.get() {
                    let summary = data.monitor_summary.clone();
                    let performance = data.performance.clone();
                    let incidents = data.incident_summary.recent.clone();
                    let open_incidents = data.incident_summary.open_count;
                    let trend = performance.trend.clone();
                    let uptime = performance.uptime_percentage.map(|v| format!("{}%", format_number(v))).unwrap_or_else(|| "—".to_string());
                    let latency = performance.avg_latency_ms.map(|v| format!("{} ms", format_number(v))).unwrap_or_else(|| "—".to_string());
                    let incident_total_24h: u32 = trend.iter().map(|point| point.incident_count).sum();
                    let spark_uptime = trend.iter().filter_map(|p| p.uptime_percentage).collect::<Vec<_>>();
                    let _spark_latency = trend.iter().filter_map(|p| p.avg_latency_ms).collect::<Vec<_>>();
                    let spark_incidents = trend.iter().map(|p| p.incident_count as f64).collect::<Vec<_>>();
                    let spark_monitors = vec![summary.total as f64; trend.len().max(2)];
                    let up_delta = trend_delta(&trend, "uptime");
                    let incident_delta = trend_delta(&trend, "incidents");

                    view! {
                        <div class="dashboard-content">
                            {move || error.get().map(|message| view! {
                                <div class="dashboard-refresh-error" role="alert">
                                    <span>{message}</span>
                                    <button class="btn" on:click=move |_| on_retry.clone().run(()) disabled=move || refreshing.get()>"Retry"</button>
                                </div>
                            })}

                            <section class="metric-strip" aria-label="Monitor health summary">
                                <div class="card metric-card metric-primary">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-monitors" aria-hidden="true">"◈"</span><span class="card-label">"MONITORS"</span></div>
                                    <span class="card-value">{summary.total}</span>
                                    <span class="card-hint"><span class="metric-trend-copy">"Configured"</span></span>
                                    <StatSparkline values=spark_monitors.clone()/>
                                </div>
                                <div class="card metric-card metric-up">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-up" aria-hidden="true">"✓"</span><span class="card-label">"UP"</span></div>
                                    <span class="card-value up">{summary.up}</span>
                                    <span class="card-hint"><TrendIndicator delta=up_delta.unwrap_or(0.0) /><span>"vs 24h ago"</span></span>
                                    <StatSparkline values=spark_uptime/>
                                </div>
                                <div class="card metric-card metric-down">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-down" aria-hidden="true">"!"</span><span class="card-label">"DOWN"</span></div>
                                    <span class="card-value down">{summary.down}</span>
                                    <span class="card-hint"><TrendIndicator delta=incident_delta.unwrap_or(0.0) inverse=true/><span>"vs 24h ago"</span></span>
                                    <StatSparkline values=spark_incidents.clone()/>
                                </div>
                                <div class="card metric-card metric-disabled">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-disabled" aria-hidden="true">"–"</span><span class="card-label">"DISABLED"</span></div>
                                    <span class="card-value">{summary.disabled}</span>
                                    <span class="card-hint">"Not monitored"</span>
                                    <StatSparkline values=spark_monitors.clone()/>
                                </div>
                                <div class="card metric-card metric-incident">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-incident" aria-hidden="true">"!"</span><span class="card-label">"INCIDENTS"</span></div>
                                    <span class="card-value">{open_incidents}</span>
                                    <span class="card-hint"><TrendIndicator delta=incident_delta.unwrap_or(0.0) inverse=true/><span>{format!("{} started in 24h", incident_total_24h)}</span></span>
                                    <StatSparkline values=spark_incidents.clone()/>
                                </div>
                            </section>

                            <section class="dashboard-section">
                                <div class="section-heading"><div><p class="eyebrow">"PERFORMANCE · 24H"</p><h2>"Performance"</h2></div></div>
                                <div class="dashboard-trend-grid">
                                    <TrendChart title="Uptime" value=uptime description="Observed successful checks" trend=trend.clone() metric="uptime" unit=""/>
                                    <TrendChart title="Response time" value=latency description="Observed check latency" trend=trend.clone() metric="latency" unit=""/>
                                    <TrendChart title="Incident activity" value=incident_total_24h.to_string() description="Incidents started in the last 24 hours" trend=trend.clone() metric="incidents" unit=""/>
                                </div>
                            </section>

                            <section class="dashboard-section">
                                <div class="section-heading dashboard-section-heading"><div><p class="eyebrow">"MONITORS"</p><h2>"Monitor health"</h2></div><A href="/monitors">"View all"</A></div>
                                <div class="monitor-toolbar">
                                    <div class="monitor-tabs" role="tablist" aria-label="Monitor status filter">
                                        {[
                                            ("all", format!("All ({})", summary.total)),
                                            ("up", format!("Up ({})", summary.up)),
                                            ("down", format!("Down ({})", summary.down)),
                                            ("disabled", format!("Disabled ({})", summary.disabled)),
                                        ].into_iter().map(|(key, label)| {
                                            let active = key.to_string();
                                            view! { <button type="button" class:active=move || monitor_filter.get() == active class="monitor-tab" on:click=move |_| monitor_filter.set(key.to_string()) role="tab">{label}</button> }
                                        }).collect_view()}
                                    </div>
                                    <input class="monitor-search" type="search" placeholder="Search monitors…" aria-label="Search monitors" prop:value=monitor_search on:input=move |event| monitor_search.set(event_target_value(&event))/>
                                </div>
                                {move || {
                                    let filter = monitor_filter.get();
                                    let search = monitor_search.get().trim().to_ascii_lowercase();
                                    let monitors = data.monitors.iter().filter(|monitor| {
                                        let status = monitor_status(monitor);
                                        let matches_filter = filter == "all" || status == filter;
                                        let matches_search = search.is_empty() || monitor.name.to_ascii_lowercase().contains(&search) || monitor.url.to_ascii_lowercase().contains(&search);
                                        matches_filter && matches_search
                                    }).take(RECENT_MONITORS_LIMIT).cloned().collect::<Vec<_>>();
                                    if monitors.is_empty() {
                                        view! { <div class="dashboard-empty compact"><p>"No monitors match this filter."</p></div> }.into_any()
                                    } else {
                                        view! { <div class="monitor-table-wrap"><table class="dashboard-monitor-table"><thead><tr><th>"Monitor"</th><th>"Status"</th><th>"Latency"</th><th>"Last check"</th><th></th></tr></thead><tbody>{monitors.into_iter().map(|monitor| {
                                            let id = monitor.id.clone();
                                            let status = monitor_status(&monitor);
                                            let (type_class, type_label) = monitor_type(&monitor.kind, &monitor.url);
                                            let latency = monitor.latency_ms.map(|v| format!("{v} ms")).unwrap_or_else(|| "—".to_string());
                                            let href = format!("/monitors/{id}");
                                            let checked = relative_time(monitor.checked_at.as_deref(), relative_now.get());
                                            view! { <tr class="monitor-row">
                                                <td><div class="monitor-cell"><span class=format!("monitor-type-icon monitor-type-{type_class}") aria-hidden="true"></span><A href=href.clone() attr:class="monitor-link"><strong>{monitor.name.clone()}</strong><small>{type_label}</small></A></div></td>
                                                <td><StatusBadge status=status/></td><td class="mono">{latency}</td><td class="mono relative-time">{checked}</td>
                                                <td class="monitor-row-end"><details class="row-menu"><summary aria-label="Monitor actions">"•••"</summary><div class="row-menu-popover"><A href=href.clone()>"Open monitor"</A><A href=format!("/monitors/{id}/edit")>"Edit"</A></div></details><A href=href attr:class="row-chevron" attr:aria-label="Open monitor">"›"</A></td>
                                            </tr> }
                                        }).collect_view()}</tbody></table></div> }.into_any()
                                    }
                                }}
                            </section>

                            <section class="dashboard-section">
                                <div class="section-heading dashboard-section-heading"><div><p class="eyebrow">"INCIDENTS"</p><h2>"Recent incidents"</h2></div><A href="/incidents">"View all"</A></div>
                                {if incidents.is_empty() { view! { <div class="dashboard-empty compact"><p>"No recent incidents."</p></div> }.into_any() } else { view! { <div class="incident-list dashboard-incident-list">{incidents.into_iter().map(|incident| {
    let incident_sparkline = spark_incidents.clone();
                                    let href = format!("/incidents/{}", incident.id);
                                    let _monitor_href = format!("/monitors/{}", incident.monitor_id);
                                    let resolved = incident.resolved_at.is_some();
                                    let status_label = if resolved { "Resolved" } else { "Investigating" };
                                    let status_class = if resolved { "incident-status-resolved" } else { "incident-status-investigating" };
                                    let when = relative_time(Some(&incident.started_at), relative_now.get());
                                    view! { <A href=href attr:class="incident-row incident-row-link">
                                        <span class=format!("incident-status-icon {}", status_class) aria-hidden="true">{if resolved { "✓" } else { "!" }}</span>
                                        <span class="incident-main"><strong>{incident.monitor_name.clone()}</strong><small>{format!("{} · {}", if resolved { "Resolved" } else { "Down" }, when)}</small></span>
                                        <StatusBadge status=if resolved { "up".to_string() } else { "down".to_string() } label=status_label.to_string()/>
                                        <StatSparkline values=incident_sparkline/>
                                        <span class="row-chevron" aria-hidden="true">"›"</span>
                                    </A> }
                                }).collect_view()}</div> }.into_any()}}
                            </section>
                        </div>
                    }.into_any()
                } else {
                    view! { <ErrorState title="Dashboard unavailable" description="No dashboard data is currently available." on_retry=on_retry/> }.into_any()
                }
            }}
        </div>
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/apps/leptos-dashboard/src/pages/monitor_detail.rs")"
cat <<'EOF' > "$PROJECT/apps/leptos-dashboard/src/pages/monitor_detail.rs"
use crate::api::monitoring::{
    self, ChannelPayload, Check, EscalationPolicy, Incident, MaintenanceWindow, Monitor,
    NotificationChannel,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::empty_state::EmptyState;
use crate::components::metric_card::MetricCard;
use crate::components::page_header::PageHeader;
use crate::components::polling;
use crate::components::status_badge::StatusBadge;
use crate::components::table_shell::TableShell;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};

fn status_from_checks(monitor: &Monitor, checks: &[Check]) -> String {
    if !monitor.enabled {
        return "disabled".into();
    }
    checks
        .first()
        .map(|check| check.status.clone())
        .unwrap_or_else(|| "unknown".into())
}

fn uptime_from_checks(checks: &[Check]) -> Option<f64> {
    if checks.is_empty() {
        return None;
    }
    let up = checks
        .iter()
        .filter(|check| check.status.eq_ignore_ascii_case("up"))
        .count();
    Some((up as f64 / checks.len() as f64) * 100.0)
}

fn average_latency(checks: &[Check]) -> Option<f64> {
    if checks.is_empty() {
        None
    } else {
        Some(checks.iter().map(|check| check.latency_ms).sum::<u128>() as f64 / checks.len() as f64)
    }
}

fn incident_status(incident: &Incident) -> &'static str {
    if incident.resolved_at.is_some() {
        "resolved"
    } else if incident.acknowledged_at.is_some() {
        "acknowledged"
    } else {
        "open"
    }
}

fn kind_label(kind: &str) -> &str {
    match kind {
        "http" | "Http" => "HTTP",
        "tcp" | "Tcp" => "TCP",
        "dns" | "Dns" => "DNS",
        "heartbeat" | "Heartbeat" => "Heartbeat",
        "flow" | "Flow" => "API flow",
        _ => kind,
    }
}

#[component]
pub fn MonitorDetailPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let monitor_id = move || params.get().get("id").unwrap_or_default();

    let monitor = RwSignal::new(None::<Monitor>);
    let checks = RwSignal::new(Vec::<Check>::new());
    let incidents = RwSignal::new(Vec::<Incident>::new());
    let channels = RwSignal::new(Vec::<NotificationChannel>::new());
    let maintenance_windows = RwSignal::new(Vec::<MaintenanceWindow>::new());
    let escalation_policies = RwSignal::new(Vec::<EscalationPolicy>::new());

    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let action_busy = RwSignal::new(false);
    let one_time_busy = RwSignal::new(false);
    let telemetry_in_flight = RwSignal::new(false);
    let show_delete_confirm = RwSignal::new(false);
    let channel_to_delete = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    Effect::new(move |_| {
        let query = query.get();
        if query.get("saved").is_some() {
            notice.set(Some("Monitor saved successfully.".into()));
        } else if query.get("created").is_some() {
            notice.set(Some("Monitor created successfully.".into()));
        }
    });

    let load_data = move || {
        let id = monitor_id();
        if id.is_empty() {
            error.set(Some("Invalid monitor URL.".into()));
            loading.set(false);
            return;
        }

        if telemetry_in_flight.get_untracked() {
            return;
        }
        loading.set(true);
        error.set(None);
        telemetry_in_flight.set(true);
        spawn_local(async move {
            // Keep the existing per-resource endpoints. They are already tenant
            // scoped server-side; this page only composes them for the detail view.
            let monitor_result = monitoring::get_monitor(&id).await;
            match monitor_result {
                Ok(value) => monitor.set(Some(value)),
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                    telemetry_in_flight.set(false);
                    return;
                }
            }

            match monitoring::fetch_checks(&id).await {
                Ok(value) => checks.set(value),
                Err(e) => error.set(Some(format!("Checks: {e}"))),
            }
            match monitoring::fetch_incidents(&id).await {
                Ok(value) => incidents.set(value),
                Err(e) => error.set(Some(format!("Incidents: {e}"))),
            }
            match monitoring::list_notification_channels(&id).await {
                Ok(value) => channels.set(value),
                Err(e) => error.set(Some(format!("Notifications: {e}"))),
            }
            match monitoring::list_maintenance_windows(&id).await {
                Ok(value) => maintenance_windows.set(value),
                Err(e) => error.set(Some(format!("Maintenance: {e}"))),
            }
            match monitoring::list_escalation_policies().await {
                Ok(value) => escalation_policies.set(value),
                Err(e) => error.set(Some(format!("Escalation policies: {e}"))),
            }

            loading.set(false);
            telemetry_in_flight.set(false);
        });
    };

    Effect::new(move |_| load_data());

    // Keep only volatile telemetry on the realtime path. Notification,
    // maintenance and escalation configuration are not refetched every tick.
    polling::start(move || {
        let id = monitor_id();
        async move {
            if id.is_empty() || telemetry_in_flight.get_untracked() {
                return Ok(());
            }
            telemetry_in_flight.set(true);

            let monitor_result = monitoring::get_monitor(&id).await;
            let checks_result = monitoring::fetch_checks(&id).await;
            let incidents_result = monitoring::fetch_incidents(&id).await;
            let mut failed = false;

            match monitor_result {
                Ok(value) => monitor.set(Some(value)),
                Err(e) => {
                    error.set(Some(e));
                    failed = true;
                }
            }
            match checks_result {
                Ok(value) => checks.set(value),
                Err(e) => {
                    error.set(Some(format!("Checks: {e}")));
                    failed = true;
                }
            }
            match incidents_result {
                Ok(value) => incidents.set(value),
                Err(e) => {
                    error.set(Some(format!("Incidents: {e}")));
                    failed = true;
                }
            }

            if !failed {
                error.set(None);
            }
            telemetry_in_flight.set(false);
            if failed {
                Err(())
            } else {
                Ok(())
            }
        }
    });

    let navigate_for_delete = navigate.clone();
    let confirm_delete_cb = Callback::new(move |_: ()| {
        if action_busy.get() {
            return;
        }
        let id = monitor_id();
        let navigate = navigate_for_delete.clone();
        action_busy.set(true);
        show_delete_confirm.set(false);
        spawn_local(async move {
            match monitoring::delete_monitor(&id).await {
                Ok(()) => navigate("/monitors", Default::default()),
                Err(e) => {
                    action_busy.set(false);
                    error.set(Some(e));
                }
            }
        });
    });

    let cancel_delete_cb = Callback::new(move |_: ()| {
        show_delete_confirm.set(false);
    });

    let run_one_time = move |_| {
        if action_busy.get() || one_time_busy.get() { return; }
        let id = monitor_id(); if id.is_empty() { return; }
        one_time_busy.set(true); error.set(None);
        spawn_local(async move {
            match monitoring::run_one_time_check(&id).await {
                Ok(_) => notice.set(Some("One-time check queued; recurring schedule is unchanged.".into())),
                Err(e) => error.set(Some(e)),
            }
            one_time_busy.set(false);
        });
    };

    let toggle_enabled = move |_| {
        if action_busy.get() {
            return;
        }
        let id = monitor_id();
        let current = monitor.get().map(|m| m.enabled).unwrap_or(false);
        action_busy.set(true);
        spawn_local(async move {
            match monitoring::set_monitor_enabled(&id, !current).await {
                Ok(()) => {
                    notice.set(Some(if current {
                        "Monitor disabled.".into()
                    } else {
                        "Monitor enabled.".into()
                    }));
                    action_busy.set(false);
                    load_data();
                }
                Err(e) => {
                    action_busy.set(false);
                    error.set(Some(e));
                }
            }
        });
    };

    let acknowledge = move |incident_id: String| {
        if action_busy.get() {
            return;
        }
        action_busy.set(true);
        spawn_local(async move {
            match monitoring::acknowledge_incident(&incident_id).await {
                Ok(()) => {
                    notice.set(Some("Incident acknowledged.".into()));
                    action_busy.set(false);
                    load_data();
                }
                Err(e) => {
                    action_busy.set(false);
                    error.set(Some(e));
                }
            }
        });
    };

    let send_test = move |channel_id: String| {
        if action_busy.get() {
            return;
        }
        action_busy.set(true);
        spawn_local(async move {
            match monitoring::test_notification_channel(&channel_id).await {
                Ok(()) => notice.set(Some("Test notification sent.".into())),
                Err(e) => error.set(Some(e)),
            }
            action_busy.set(false);
        });
    };

    let ch_kind = RwSignal::new(String::new());
    let ch_target = RwSignal::new(String::new());
    let ch_chat_id = RwSignal::new(String::new());
    let channel_error = RwSignal::new(None::<String>);
    let channel_secret = RwSignal::new(None::<String>);

    let create_channel = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        channel_error.set(None);
        let id = monitor_id();
        let kind = ch_kind.get().trim().to_string();
        let target = ch_target.get().trim().to_string();
        let chat_id = ch_chat_id.get().trim().to_string();

        if kind.is_empty() {
            channel_error.set(Some("Choose a notification type.".into()));
            return;
        }
        if target.is_empty() {
            channel_error.set(Some("Notification target is required.".into()));
            return;
        }
        if kind == "telegram" && chat_id.is_empty() {
            channel_error.set(Some("Telegram chat ID is required.".into()));
            return;
        }
        if action_busy.get() {
            return;
        }

        action_busy.set(true);
        let payload = ChannelPayload {
            kind,
            target,
            chat_id: if chat_id.is_empty() {
                None
            } else {
                Some(chat_id)
            },
        };
        spawn_local(async move {
            match monitoring::create_notification_channel(&id, &payload).await {
                Ok(created) => {
                    channel_secret.set(created.signing_secret);
                    ch_kind.set(String::new());
                    ch_target.set(String::new());
                    ch_chat_id.set(String::new());
                    notice.set(Some("Notification channel created.".into()));
                    action_busy.set(false);
                    load_data();
                }
                Err(e) => {
                    action_busy.set(false);
                    channel_error.set(Some(e));
                }
            }
        });
    };

    let confirm_channel_delete = Callback::new(move |_: ()| {
        let channel_id = match channel_to_delete.get() {
            Some(id) => id,
            None => return,
        };
        if action_busy.get() {
            return;
        }
        action_busy.set(true);
        channel_to_delete.set(None);
        spawn_local(async move {
            match monitoring::delete_notification_channel(&channel_id).await {
                Ok(()) => {
                    notice.set(Some("Notification channel deleted.".into()));
                    action_busy.set(false);
                    load_data();
                }
                Err(e) => {
                    action_busy.set(false);
                    error.set(Some(e));
                }
            }
        });
    });

    let cancel_channel_delete = Callback::new(move |_: ()| {
        channel_to_delete.set(None);
    });

    view! {
        <div class="monitor-detail">
            {move || {
                if loading.get() && monitor.get().is_none() {
                    view! {
                        <div class="data-section">
                            <div class="loading">"Loading monitor…"</div>
                        </div>
                    }.into_any()
                } else if monitor.get().is_none() {
                    view! {
                        <div class="error-state">
                            <h2>"Monitor unavailable"</h2>
                            <p>{move || error.get().unwrap_or_else(|| "The monitor could not be loaded.".into())}</p>
                            <button class="btn" on:click=move |_| load_data()>"Retry"</button>
                        </div>
                    }.into_any()
                } else {
                    let m = monitor.get().unwrap_or_default();
                    let status = status_from_checks(&m, &checks.get());
                    let uptime = uptime_from_checks(&checks.get())
                        .map(|v| format!("{v:.2}%"))
                        .unwrap_or_else(|| "—".into());
                    let avg_latency = average_latency(&checks.get())
                        .map(|v| format!("{v:.0} ms"))
                        .unwrap_or_else(|| "—".into());
                    let latest = checks.get().first().cloned();
                    let ssl_status = if !m.ssl_check_enabled {
                        "Disabled".to_string()
                    } else if let Some(days) = latest.as_ref().and_then(|c| c.cert_days_remaining) {
                        if days < 0 {
                            format!("Expired ({days}d)")
                        } else if days <= m.ssl_expiry_warning_days as i64 {
                            format!("Expires in {days}d")
                        } else {
                            format!("Valid ({days}d)")
                        }
                    } else {
                        "No certificate data".into()
                    };
                    let max_latency = checks
                        .get()
                        .iter()
                        .map(|c| c.latency_ms)
                        .max()
                        .unwrap_or(1);

                    let navigate_edit = navigate.clone();
                    let monitor_id_for_edit = monitor_id();

                    view! {
                        <PageHeader title=m.name.clone()>
                            <button
                                class="btn"
                                disabled=move || action_busy.get()
                                on:click=move |_| navigate_edit(&format!("/monitors/{monitor_id_for_edit}/edit"), Default::default())
                            >"Edit"</button>
                            <button
                                class="btn"
                                disabled=move || action_busy.get()
                                on:click=toggle_enabled
                            >{if m.enabled { "Disable" } else { "Enable" }}</button>
                            <button class="btn" disabled=move || action_busy.get() || one_time_busy.get() on:click=run_one_time>{move || if one_time_busy.get() { "Queuing…" } else { "Test now" }}</button>
                            <button
                                class="btn btn-danger"
                                disabled=move || action_busy.get()
                                on:click=move |_| show_delete_confirm.set(true)
                            >"Delete"</button>
                        </PageHeader>

                        {move || error.get().map(|e| view! {
                            <div class="alert alert-error" role="alert">{e}</div>
                        })}
                        {move || notice.get().map(|n| view! {
                            <div class="alert alert-success" role="status" aria-live="polite">{n}</div>
                        })}

                        {move || if show_delete_confirm.get() {
                            view! {
                                <ConfirmDialog
                                    title="Delete monitor?"
                                    body="This permanently deletes the monitor and its recorded history. This action cannot be undone."
                                    on_confirm=confirm_delete_cb
                                    on_cancel=cancel_delete_cb
                                />
                            }.into_any()
                        } else {
                            ().into_any()
                        }}

                        {move || if channel_to_delete.get().is_some() {
                            view! {
                                <ConfirmDialog
                                    title="Delete notification channel?"
                                    body="This removes the channel from this monitor. You can create it again later."
                                    on_confirm=confirm_channel_delete
                                    on_cancel=cancel_channel_delete
                                />
                            }.into_any()
                        } else {
                            ().into_any()
                        }}

                        <section class="monitor-identity data-section">
                            <div>
                                <span class="eyebrow">"Monitor"</span>
                                <h2>{m.name.clone()}</h2>
                                <p class="muted">{format!("{} · {} · {}", kind_label(&m.kind), m.method, m.url.clone().unwrap_or_else(|| "No target".into()))}</p>
                            </div>
                            <div class="monitor-status">
                                <StatusBadge status=status.clone()/>
                            </div>
                        </section>

                        <section class="metric-strip monitor-health-metrics">
                            <MetricCard label="Current status" value=status/>
                            <MetricCard label="Uptime (last 50 checks)" value=uptime/>
                            <MetricCard label="Avg response" value=avg_latency/>
                            <MetricCard label="Check frequency" value=format!("{}s", m.interval_seconds)/>
                            <MetricCard label="SSL" value=ssl_status/>
                        </section>

                        <section class="data-section">
                            <div class="section-heading">
                                <div>
                                    <h2>"Performance"</h2>
                                    <p class="muted">"Recent response-time trend · newest check first"</p>
                                </div>
                            </div>
                            {move || if checks.get().is_empty() {
                                view! {
                                    <EmptyState title="No performance data" description="Performance data will appear after the monitor runs."/>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="performance-bars" aria-label="Recent response time chart">
                                        {checks.get().iter().take(20).enumerate().map(|(index, check)| {
                                            let latency = check.latency_ms;
                                            let width = if latency == 0 { 2 } else {
                                                ((latency as f64 / max_latency as f64) * 100.0).max(4.0) as u32
                                            };
                                            let label = format!("{} · {} ms", check.checked_at, latency);
                                            let status = check.status.clone();
                                            view! {
                                                <div class="performance-row" title=label>
                                                    <span class="performance-index">{format!("{}", index + 1)}</span>
                                                    <div class="performance-track">
                                                        <div
                                                            class=format!("performance-fill performance-{}", status.to_lowercase())
                                                            style=format!("width:{width}%")
                                                        ></div>
                                                    </div>
                                                    <span class="performance-value">{format!("{latency} ms")}</span>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Recent checks"</h2><span class="muted">{format!("{} loaded", checks.get().len())}</span></div>
                            {move || if checks.get().is_empty() {
                                view! {
                                    <EmptyState title="No checks recorded" description="Checks will appear here once this monitor runs."/>
                                }.into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Checked at".into(),
                                        "Status".into(),
                                        "Response".into(),
                                        "Region".into(),
                                        "TLS".into(),
                                        "Error".into(),
                                    ]>
                                        {checks.get().into_iter().take(20).map(|check| view! {
                                            <tr>
                                                <td>{check.checked_at}</td>
                                                <td><StatusBadge status=check.status.clone()/></td>
                                                <td>{format!("{} ms", check.latency_ms)}</td>
                                                <td>{check.region}</td>
                                                <td>{check.cert_days_remaining.map(|v| format!("{v}d")).unwrap_or_else(|| "—".into())}</td>
                                                <td>{check.error_message.unwrap_or_else(|| "—".into())}</td>
                                            </tr>
                                        }).collect_view()}
                                    </TableShell>
                                }.into_any()
                            }}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Incidents"</h2><span class="muted">{format!("{}", incidents.get().len())}</span></div>
                            {move || if incidents.get().is_empty() {
                                view! {
                                    <EmptyState title="No incidents" description="This monitor has no recorded incidents."/>
                                }.into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "First seen".into(),
                                        "Status".into(),
                                        "Duration".into(),
                                        "Last seen".into(),
                                        "Maintenance".into(),
                                        "Actions".into(),
                                    ]>
                                        {incidents.get().into_iter().map(|incident| {
                                            let id = incident.id.clone();
                                            let status = incident_status(&incident);
                                            let can_ack = status == "open";
                                            view! {
                                                <tr>
                                                    <td><A href=format!("/incidents/{}", incident.id) attr:class="incident-link">{incident.first_seen}</A></td>
                                                    <td><StatusBadge status=status.to_string()/></td>
                                                    <td>{format!("{} ms", incident.duration_ms)}</td>
                                                    <td>{incident.last_seen_at.unwrap_or_else(|| "—".into())}</td>
                                                    <td>{if incident.occurred_during_maintenance { "Yes" } else { "No" }}</td>
                                                    <td>
                                                        {if can_ack {
                                                            view! {
                                                                <button
                                                                    class="btn"
                                                                    disabled=move || action_busy.get()
                                                                    on:click=move |_| acknowledge(id.clone())
                                                                >"Acknowledge"</button>
                                                            }.into_any()
                                                        } else {
                                                            ().into_any()
                                                        }}
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </TableShell>
                                }.into_any()
                            }}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Notifications"</h2></div>
                            {move || if channels.get().is_empty() {
                                view! {
                                    <EmptyState title="No notifications configured" description="Add a channel below to receive monitor alerts."/>
                                }.into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Type".into(),
                                        "Target".into(),
                                        "Enabled".into(),
                                        "Secret".into(),
                                        "Actions".into(),
                                    ]>
                                        {channels.get().into_iter().map(|channel| {
                                            let id = channel.id.clone();
                                            let test_id = id.clone();
                                            view! {
                                                <tr>
                                                    <td>{channel.kind.clone()}</td>
                                                    <td class="mono">{channel.target.clone()}</td>
                                                    <td>{if channel.enabled { "Enabled" } else { "Disabled" }}</td>
                                                    <td>{if channel.has_signing_secret { "Configured" } else { "—" }}</td>
                                                    <td>
                                                        <button class="btn" disabled=move || action_busy.get() on:click=move |_| send_test(test_id.clone())>"Send test"</button>
                                                        <button class="btn btn-danger" disabled=move || action_busy.get() on:click=move |_| channel_to_delete.set(Some(id.clone()))>"Delete"</button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </TableShell>
                                }.into_any()
                            }}

                            <form on:submit=create_channel class="channel-form monitor-subform">
                                <select
                                    prop:value=ch_kind
                                    on:change=move |ev| ch_kind.set(event_target_value(&ev))
                                    required
                                    aria-label="Notification type"
                                >
                                    <option value="">"Notification type"</option>
                                    <option value="email">"Email"</option>
                                    <option value="webhook">"Webhook"</option>
                                    <option value="slack">"Slack"</option>
                                    <option value="discord">"Discord"</option>
                                    <option value="telegram">"Telegram"</option>
                                    <option value="sms">"SMS"</option>
                                </select>
                                <input
                                    type="text"
                                    placeholder="Target (email, webhook URL, bot token, etc.)"
                                    prop:value=ch_target
                                    on:input=move |ev| ch_target.set(event_target_value(&ev))
                                    maxlength="2048"
                                    required
                                />
                                {move || if ch_kind.get() == "telegram" {
                                    view! {
                                        <input
                                            type="text"
                                            placeholder="Telegram chat ID"
                                            prop:value=ch_chat_id
                                            on:input=move |ev| ch_chat_id.set(event_target_value(&ev))
                                            maxlength="128"
                                            required
                                        />
                                    }.into_any()
                                } else {
                                    ().into_any()
                                }}
                                <button class="btn btn-primary" type="submit" disabled=move || action_busy.get()>"Add channel"</button>
                            </form>
                            {move || channel_error.get().map(|e| view! {
                                <div class="form-error-message" role="alert">{e}</div>
                            })}
                            {move || channel_secret.get().map(|secret| view! {
                                <div class="alert alert-warning" role="alert">
                                    <strong>"Save this webhook signing secret now. "</strong>
                                    "It will not be shown again: "
                                    <code>{secret}</code>
                                </div>
                            })}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Maintenance"</h2></div>
                            {move || if maintenance_windows.get().is_empty() {
                                view! {
                                    <EmptyState title="No maintenance windows" description="No maintenance windows are configured for this monitor."/>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="list-stack">
                                        {maintenance_windows.get().into_iter().take(20).map(|window| view! {
                                            <article class="list-row">
                                                <div>
                                                    <strong>{window.reason}</strong>
                                                    <div class="muted">{format!("{} → {}", window.starts_at, window.ends_at)}</div>
                                                </div>
                                            </article>
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Escalation"</h2></div>
                            {move || {
                                let id = monitor_id();
                                let policies: Vec<EscalationPolicy> = escalation_policies
                                    .get()
                                    .into_iter()
                                    .filter(|policy| policy.monitor_id.as_deref() == Some(id.as_str()))
                                    .collect();
                                if policies.is_empty() {
                                    view! {
                                        <EmptyState title="No monitor-specific policy" description="No escalation policy is attached directly to this monitor."/>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="list-stack">
                                            {policies.into_iter().map(|policy| view! {
                                                <article class="list-row">
                                                    <div>
                                                        <strong>{format!("Policy {}", policy.id)}</strong>
                                                        <div class="muted">{format!("{} step(s)", policy.steps.len())}</div>
                                                        <ol class="escalation-steps">
                                                            {policy.steps.into_iter().enumerate().map(|(index, step)| view! {
                                                                <li>{format!("Step {} · after {} min · channel {}", index + 1, step.delay_minutes, step.channel_id)}</li>
                                                            }).collect_view()}
                                                        </ol>
                                                    </div>
                                                </article>
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </section>
                    }.into_any()
                }
            }}
        </div>
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/apps/leptos-dashboard/src/pages/pricing.rs")"
cat <<'EOF' > "$PROJECT/apps/leptos-dashboard/src/pages/pricing.rs"
use crate::api::billing;
use leptos::prelude::*; use leptos::task::spawn_local;
fn usd(u:i64)->String{format!("${}.{:06}",u/1_000_000,u.abs()%1_000_000)}
#[component] pub fn PricingPage()->impl IntoView{let prices=RwSignal::new(Vec::<billing::PaygPricing>::new());let error=RwSignal::new(None::<String>);Effect::new(move |_|{spawn_local(async move{match billing::payg_pricing().await{Ok(v)=>prices.set(v),Err(e)=>error.set(Some(e.to_string()))}})});view!{<div class="feature-page"><header class="page-header"><div><span class="eyebrow">"PAYG"</span><h1>"Pricing"</h1><p class="muted">"Server-controlled prices per actual billable unit."</p></div></header>{move||error.get().map(|e|view!{<div class="error" role="alert">{e}</div>})}<section class="card"><div class="overview-cards">{move||prices.get().into_iter().map(|p|view!{<div><span class="card-label">{p.operation}</span><span class="card-value">{usd(p.unit_price)}</span><span class="muted">{format!("per {} · v{}",p.unit_name,p.version)}</span></div>}).collect_view()}</div></section><section class="card"><h2>"Free entitlement"</h2><p class="muted">"10 monitors at a 5-minute minimum interval. A funded wallet unlocks PAYG capacity without a product monitor-count ceiling."</p></section></div>}}
EOF

mkdir -p "$(dirname "$PROJECT/apps/leptos-dashboard/src/pages/wallet.rs")"
cat <<'EOF' > "$PROJECT/apps/leptos-dashboard/src/pages/wallet.rs"
use crate::api::wallet;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn usd(units: i64) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let a = units.unsigned_abs();
    format!("{sign}${}.{:02}", a / 1_000_000, (a % 1_000_000) / 10_000)
}

#[component]
pub fn WalletPage() -> impl IntoView {
    let wallet = RwSignal::new(None::<wallet::Wallet>);
    let transactions = RwSignal::new(Vec::<wallet::WalletTransaction>::new());
    let notifications = RwSignal::new(Vec::<wallet::WalletNotification>::new());
    let settings = RwSignal::new(None::<wallet::WalletSettings>);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let amount = RwSignal::new(25_i64);
    let loading = RwSignal::new(false);
    let auto_enabled = RwSignal::new(false);
    let threshold = RwSignal::new(5_i64);
    let auto_amount = RwSignal::new(20_i64);
    let payment_method = RwSignal::new(String::new());
    let daily_limit = RwSignal::new(100_i64);
    let monthly_limit = RwSignal::new(500_i64);
    let retry_limit = RwSignal::new(3_i32);
    let low_threshold = RwSignal::new(5_i64);
    let critical_threshold = RwSignal::new(1_i64);
    let refund_payment_id = RwSignal::new(String::new());
    let refund_amount = RwSignal::new(0_i64);

    let reload = move || {
        spawn_local(async move {
            match wallet::get().await {
                Ok(v) => wallet.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
            match wallet::transactions().await {
                Ok(v) => transactions.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            match wallet::notifications().await {
                Ok(v) => notifications.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            match wallet::settings().await {
                Ok(v) => {
                    auto_enabled.set(v.auto_top_up_enabled);
                    threshold.set(v.auto_top_up_threshold_units / 1_000_000);
                    auto_amount.set(v.auto_top_up_amount_units / 1_000_000);
                    payment_method.set(v.payment_method.clone().unwrap_or_default());
                    daily_limit.set(v.daily_top_up_limit_units / 1_000_000);
                    monthly_limit.set(v.monthly_top_up_limit_units / 1_000_000);
                    retry_limit.set(v.retry_limit);
                    low_threshold.set(v.low_balance_threshold_units / 1_000_000);
                    critical_threshold.set(v.critical_balance_threshold_units / 1_000_000);
                    settings.set(Some(v));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    Effect::new(move |_| reload());

    let save_settings = move |_| {
        error.set(None);
        notice.set(None);

        let low = low_threshold.get().max(0);
        let critical = critical_threshold.get().max(0);
        let auto_trigger = threshold.get().max(0);
        let auto_top_up_amount = auto_amount.get().max(5);
        let daily = daily_limit.get().max(1);
        let monthly = monthly_limit.get().max(1);
        let retries = retry_limit.get().clamp(0, 10);
        let payment = payment_method.get().trim().to_owned();

        if critical > low {
            error.set(Some(
                "Critical warning must be less than or equal to the low-balance warning."
                    .to_owned(),
            ));
            return;
        }

        if auto_enabled.get() && payment.is_empty() {
            error.set(Some(
                "A saved payment method is required when auto top-up is enabled.".to_owned(),
            ));
            return;
        }

        if auto_enabled.get() && auto_trigger <= 0 {
            error.set(Some(
                "Auto top-up trigger must be greater than $0 when auto top-up is enabled."
                    .to_owned(),
            ));
            return;
        }

        if daily < auto_top_up_amount {
            error.set(Some(
                "Daily top-up limit must be at least the auto top-up amount.".to_owned(),
            ));
            return;
        }

        if monthly < daily {
            error.set(Some(
                "Monthly top-up limit must be at least the daily top-up limit.".to_owned(),
            ));
            return;
        }

        loading.set(true);

        let req = wallet::WalletSettingsRequest {
            low_balance_threshold_units: low * 1_000_000,
            critical_balance_threshold_units: critical * 1_000_000,
            auto_top_up_enabled: auto_enabled.get(),
            auto_top_up_threshold_units: auto_trigger * 1_000_000,
            auto_top_up_amount_units: auto_top_up_amount * 1_000_000,
            payment_method: (!payment.is_empty()).then_some(payment),
            daily_top_up_limit_units: daily * 1_000_000,
            monthly_top_up_limit_units: monthly * 1_000_000,
            retry_limit: retries,
        };

        spawn_local(async move {
            match wallet::update_settings(&req).await {
                Ok(v) => {
                    auto_enabled.set(v.auto_top_up_enabled);
                    threshold.set(v.auto_top_up_threshold_units / 1_000_000);
                    auto_amount.set(v.auto_top_up_amount_units / 1_000_000);
                    payment_method.set(v.payment_method.clone().unwrap_or_default());
                    daily_limit.set(v.daily_top_up_limit_units / 1_000_000);
                    monthly_limit.set(v.monthly_top_up_limit_units / 1_000_000);
                    retry_limit.set(v.retry_limit);
                    low_threshold.set(v.low_balance_threshold_units / 1_000_000);
                    critical_threshold.set(v.critical_balance_threshold_units / 1_000_000);
                    settings.set(Some(v));
                    notice.set(Some("Wallet settings saved successfully.".to_owned()));
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                }
            }

            loading.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"BILLING"</span><h1>"Wallet"</h1><p class="muted">"Prepaid PAYG balance for metered operations."</p></div></header>
            {move||error.get().map(|e|view!{<div class="error" role="alert">{e}</div>})}
            {move||notice.get().map(|n|view!{<div class="success" role="status">{n}</div>})}

            {move||wallet.get().map(|w|view!{<section class="card">
                <div class="section-heading"><div><h2>"Wallet status"</h2><p class="muted">"PAYG stops safely at zero; thresholds only control warnings and automation."</p></div><strong>{w.status.clone()}</strong></div>
                <div class="overview-cards"><div><span class="card-label">"Balance"</span><span class="card-value">{usd(w.balance_units)}</span></div><div><span class="card-label">"Low"</span><span class="card-value">{usd(w.low_balance_threshold_units)}</span></div><div><span class="card-label">"Critical"</span><span class="card-value">{usd(w.critical_balance_threshold_units)}</span></div></div>
                {move || {
                    if w.status == "WARNING" {
                        view! {
                            <div class="notice">
                                "Your PAYG balance is low. Consider topping up."
                            </div>
                        }.into_any()
                    } else if w.status == "CRITICAL" {
                        view! {
                            <div class="warning">
                                "Your PAYG balance is critically low."
                            </div>
                        }.into_any()
                    } else if w.status == "BLOCKED" {
                        view! {
                            <div class="error">
                                "PAYG is blocked because your balance is zero."
                            </div>
                        }.into_any()
                    } else {
                        ().into_any()
                    }
                }}
            </section>})}

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Top up"</h2>
                        <p class="muted">"Choose an amount and continue to the secure payment provider."</p>
                    </div>
                </div>

                <div class="stack-form">
                    <div>
                        <span class="card-label">"Quick amount"</span>
                        <div class="inline-form">
                            {
                                [5_i64, 10, 25, 50, 100]
                                    .into_iter()
                                    .map(|v| {
                                        let selected = move || {
                                            if amount.get() == v {
                                                "btn btn-primary"
                                            } else {
                                                "btn"
                                            }
                                        };

                                        view! {
                                            <button
                                                type="button"
                                                class=selected
                                                disabled=move || loading.get()
                                                on:click=move |_| amount.set(v)
                                            >
                                                {format!("${v}")}
                                            </button>
                                        }
                                    })
                                    .collect_view()
                            }
                        </div>
                    </div>

                    <label>
                        "Custom amount ($)"
                        <input
                            type="number"
                            min="5"
                            
                            step="1"
                            prop:value=move || amount.get().to_string()
                            on:input=move |e| {
                                if let Ok(v) = event_target_value(&e).parse::<i64>() {
                                    amount.set(v.max(5));
                                }
                            }
                            disabled=move || loading.get()
                        />
                    </label>

                    <div class="inline-form">
                        <span class="muted">
                            {move || format!("Selected: ${}.00", amount.get().max(5))}
                        </span>

                        <button
                            type="button"
                            class="btn btn-primary"
                            disabled=move || loading.get()
                            on:click=move |_| {
                                loading.set(true);
                                error.set(None);

                                let a = amount.get().max(5) * 1_000_000;

                                spawn_local(async move {
                                    match wallet::top_up(&wallet::TopUpRequest {
                                        amount_units: a,
                                        payment_method: None,
                                    })
                                    .await
                                    {
                                        Ok(v) => {
                                            if let Some(w) = web_sys::window() {
                                                let _ = w.location().set_href(&v.checkout_url);
                                            } else {
                                                error.set(Some(
                                                    "Unable to open the payment provider."
                                                        .to_owned(),
                                                ));
                                                loading.set(false);
                                            }
                                        }
                                        Err(e) => {
                                            error.set(Some(e.to_string()));
                                            loading.set(false);
                                        }
                                    }
                                });
                            }
                        >
                            {move || if loading.get() {
                                "Opening payment…"
                            } else {
                                "Continue to Payment"
                            }}
                        </button>
                    </div>

                    <p class="muted">
                        "Minimum $5.00. There is no product-defined maximum; provider and risk controls may apply. Your wallet is credited only after the provider webhook is verified."
                    </p>
                </div>
            </section>

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Auto top-up"</h2>
                        <p class="muted">
                            "Automatically replenish the wallet when the balance falls below the configured trigger."
                        </p>
                    </div>

                    <strong>
                        {move || {
                            if auto_enabled.get() {
                                "ENABLED"
                            } else {
                                "DISABLED"
                            }
                        }}
                    </strong>
                </div>

                <div class="stack-form">
                    <label class="check-row">
                        <input
                            type="checkbox"
                            prop:checked=move || auto_enabled.get()
                            disabled=move || loading.get()
                            on:change=move |event| {
                                auto_enabled.set(
                                    event_target::<web_sys::HtmlInputElement>(&event).checked()
                                );
                            }
                        />
                        " Enable auto top-up"
                    </label>

                    <div class="inline-form">
                        <label>
                            "Trigger below ($)"
                            <input
                                type="number"
                                min="0"
                                prop:value=move || threshold.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        threshold.set(v.max(0));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Top-up amount ($)"
                            <input
                                type="number"
                                min="5"
                                
                                prop:value=move || auto_amount.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        auto_amount.set(v.max(5));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Daily limit ($)"
                            <input
                                type="number"
                                min="1"
                                prop:value=move || daily_limit.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        daily_limit.set(v.max(1));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Monthly limit ($)"
                            <input
                                type="number"
                                min="1"
                                prop:value=move || monthly_limit.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        monthly_limit.set(v.max(1));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Retry limit"
                            <input
                                type="number"
                                min="0"
                                max="10"
                                prop:value=move || retry_limit.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i32>()
                                    {
                                        retry_limit.set(v.clamp(0, 10));
                                    }
                                }
                            />
                        </label>
                    </div>

                    <label>
                        "Saved payment method ID"
                        <input
                            type="password"
                            autocomplete="off"
                            prop:value=move || payment_method.get()
                            disabled=move || loading.get()
                            on:input=move |event| {
                                payment_method.set(event_target_value(&event));
                            }
                        />
                    </label>

                    <p class="muted">
                        "A saved payment method is required to enable automatic charging. Daily, monthly and retry limits are enforced server-side."
                    </p>

                    <div class="inline-form">
                        <label>
                            "Low warning ($)"
                            <input
                                type="number"
                                min="0"
                                prop:value=move || low_threshold.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        low_threshold.set(v.max(0));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Critical warning ($)"
                            <input
                                type="number"
                                min="0"
                                prop:value=move || critical_threshold.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        critical_threshold.set(v.max(0));
                                    }
                                }
                            />
                        </label>

                        <button
                            class="btn btn-primary"
                            disabled=move || loading.get()
                            on:click=save_settings
                        >
                            {move || {
                                if loading.get() {
                                    "Saving…"
                                } else {
                                    "Save settings"
                                }
                            }}
                        </button>
                    </div>
                </div>
            </section>

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Wallet notifications"</h2>
                        <p class="muted">"Low-balance, critical-balance and payment events for this wallet."</p>
                    </div>
                </div>
                {move || if notifications.get().is_empty() {
                    view! { <p class="muted">"No wallet notifications yet."</p> }.into_any()
                } else {
                    view! {
                        <div class="stack-form">
                            {move || notifications.get().into_iter().map(|n| view! {
                                <div class="notice">
                                    <strong>{n.event_type}</strong>
                                    <div>{n.message}</div>
                                    <small class="muted">{n.created_at}</small>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Request a refund"</h2>
                        <p class="muted">"Refund a verified wallet top-up. Refunds are processed by the payment provider and finalized by webhook verification."</p>
                    </div>
                </div>
                <div class="stack-form">
                    <label>
                        "Provider payment ID"
                        <input
                            type="text"
                            autocomplete="off"
                            prop:value=move || refund_payment_id.get()
                            on:input=move |event| refund_payment_id.set(event_target_value(&event))
                            disabled=move || loading.get()
                        />
                    </label>
                    <label>
                        "Refund amount ($)"
                        <input
                            type="number"
                            min="0.01"
                            max="1000"
                            step="0.01"
                            prop:value=move || {
                                let units = refund_amount.get();
                                format!("{}.{:02}", units / 1_000_000, (units % 1_000_000) / 10_000)
                            }
                            on:input=move |event| {
                                let raw = event_target_value(&event);
                                if let Ok(value) = raw.parse::<f64>() {
                                    refund_amount.set((value * 1_000_000.0).round() as i64);
                                }
                            }
                            disabled=move || loading.get()
                        />
                    </label>
                    <button
                        type="button"
                        class="btn btn-primary"
                        disabled=move || loading.get() || refund_payment_id.get().trim().is_empty() || refund_amount.get() <= 0
                        on:click=move |_| {
                            error.set(None);
                            notice.set(None);
                            loading.set(true);
                            let req = wallet::RefundRequest {
                                provider_payment_id: refund_payment_id.get().trim().to_owned(),
                                amount_units: refund_amount.get(),
                                currency: Some("USD".to_owned()),
                            };
                            spawn_local(async move {
                                match wallet::refund(&req).await {
                                    Ok(v) => {
                                        notice.set(Some(format!("Refund {} submitted; provider status: {}.", v.provider_refund_id, v.status)));
                                        refund_payment_id.set(String::new());
                                        refund_amount.set(0);
                                    }
                                    Err(e) => error.set(Some(e.to_string())),
                                }
                                loading.set(false);
                            });
                        }
                    >
                        {move || if loading.get() { "Submitting…" } else { "Submit refund" }}
                    </button>
                    <p class="muted">"Refunds require billing permission and must match a refundable payment owned by this tenant."</p>
                </div>
            </section>

            <section class="card"><div class="section-heading"><h2>"Recent transactions"</h2><a class="btn" href="/billing/transactions">"View all transactions"</a></div><div class="table-shell"><table class="data-table"><thead><tr><th>"Date"</th><th>"Type"</th><th>"Description"</th><th>"Amount"</th><th>"Balance"</th></tr></thead><tbody>{move||transactions.get().into_iter().map(|t|view!{<tr><td>{t.created_at}</td><td>{t.transaction_type}</td><td>{t.description.unwrap_or_default()}</td><td>{usd(t.amount_units)}</td><td>{usd(t.balance_after_units)}</td></tr>}).collect_view()}</tbody></table></div></section>
        </div>
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/api/src/handlers/create_monitor.rs")"
cat <<'EOF' > "$PROJECT/crates/api/src/handlers/create_monitor.rs"
use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use tracing::info;

use application::{
    dto::CreateMonitorRequest, use_cases::CreateMonitorUseCase,
    use_cases::create_monitor::CreateMonitorError,
};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::{MonitorCreateResponse, MonitorWriteRequest},
    state::AppState,
};

/// Creates a monitor through the API.
pub async fn create_monitor(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<MonitorWriteRequest>,
) -> Result<(StatusCode, Json<MonitorCreateResponse>), ApiError> {
    info!("API: create_monitor");

    let request = CreateMonitorRequest {
        name: payload.name,
        url: payload.url,
        method: payload.method,
        expected_status: payload.expected_status,
        interval_seconds: payload.interval_seconds,
        timeout_seconds: payload.timeout_seconds,
        assertions: payload.assertions,
        is_public: payload.is_public,
        ssl_check_enabled: payload.ssl_check_enabled,
        ssl_expiry_warning_days: payload.ssl_expiry_warning_days,
        kind: payload.kind,
        tcp_port: payload.tcp_port,
        dns_record_type: payload.dns_record_type,
        dns_expected_value: payload.dns_expected_value,
        heartbeat_interval_seconds: payload.heartbeat_interval_seconds,
        heartbeat_grace_period_seconds: payload.heartbeat_grace_period_seconds,
    };

    let use_case = CreateMonitorUseCase::new(
        state.monitor_repository.clone(),
        state.id_generator,
        state.audit_log_entry_repository.clone(),
    )
    .with_wallet_repository(state.wallet_repository.clone());

    let monitor = use_case
        .execute(request, authenticated_user)
        .await
        .map_err(|err| match err {
            CreateMonitorError::Forbidden => {
                ApiError::forbidden("you do not have permission to create monitors")
            }
            CreateMonitorError::QuotaExceeded { limit } => ApiError::too_many_requests(format!(
                "monitor limit reached: your plan allows at most {limit} monitors"
            )),
            CreateMonitorError::TooManyAssertions { limit } => ApiError::bad_request(format!(
                "too many assertions: a monitor allows at most {limit} assertions"
            )),
            CreateMonitorError::IntervalBelowPlanMinimum { min } => ApiError::bad_request(format!(
                "interval must be at least {min} seconds on your current plan"
            )),
            CreateMonitorError::Other(err) => {
                let msg = err.to_string();
                let is_validation_error = msg.contains("invalid")
                    || msg.contains("cannot")
                    || msg.contains("must")
                    || msg.contains("required")
                    || msg.contains("interval")
                    || msg.contains("timeout")
                    || msg.contains("expected")
                    || msg.contains("url")
                    || msg.contains("host")
                    || msg.contains("dns")
                    || msg.contains("heartbeat");

                if is_validation_error {
                    ApiError::bad_request(msg)
                } else {
                    ApiError::internal_with_source("failed to create monitor", err)
                }
            }
        })?;

    Ok((
        StatusCode::CREATED,
        Json(MonitorCreateResponse::from_created(monitor)),
    ))
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/api/src/handlers/heartbeat.rs")"
cat <<'EOF' > "$PROJECT/crates/api/src/handlers/heartbeat.rs"
use std::time::{Duration, Instant};

use application::MeterUsageInput;
use application::use_cases::ChargeForUsageError;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use chrono::Utc;
use domain::value_objects::HeartbeatToken;
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::state::{AppState, HeartbeatRateLimiter};

/// Max number of `GET /public/heartbeat/{token}` requests a single token
/// may make within `HEARTBEAT_RATE_LIMIT_WINDOW`.
///
/// This is a PER-TOKEN limit, deliberately separate from the IP-based
/// `GovernorLayer` applied to the rest of `public::router()` — a heartbeat
/// token is typically pinged repeatedly from the SAME source IP (the
/// monitored service's own cron/health-check job), so an IP-based limit
/// alone would either be too loose (one IP hosting many monitored
/// services, e.g. a shared CI runner) or too tight (one IP legitimately
/// pinging many of its own heartbeat monitors). Keying on the token itself
/// gives each monitored service its own independent quota regardless of
/// source IP.
const HEARTBEAT_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const HEARTBEAT_RATE_LIMIT_MAX_REQUESTS: u32 = 6;
const MAX_HEARTBEAT_IDEMPOTENCY_KEY_BYTES: usize = 128;

/// `GET /api/v1/public/heartbeat/{token}`
///
/// Unauthenticated "dead man's switch" ping endpoint for
/// `MonitorKind::Heartbeat` monitors. A monitored service (cron job,
/// background worker, etc.) calls this on its own schedule; the worker's
/// scheduler separately evaluates whether a heartbeat has been received
/// recently enough (see `evaluate_heartbeat_monitor` in
/// `worker::scheduler`) and opens/resolves an incident accordingly.
///
/// SECURITY: never returns any monitor details (name, URL, owner, etc.) —
/// only a generic acknowledgement — since this endpoint is unauthenticated
/// and reachable by anyone who has (or guesses/leaks) a token.
///
/// B34: the raw path segment is validated as a well-formed
/// `HeartbeatToken` BEFORE it is ever used as a key into the in-memory
/// rate-limit map. Previously, any arbitrary string reaching this handler
/// — including garbage that could never match a real monitor — was
/// inserted into `HeartbeatRateLimiter` first, and only rejected by the
/// DB lookup afterwards. Since that map is never pruned (see its doc
/// comment) and is keyed by the raw path string, an attacker could grow
/// it without bound by requesting a large number of distinct nonsense
/// paths — a memory-exhaustion vector on an unauthenticated endpoint,
/// independent of (and not mitigated by) the IP-based `GovernorLayer` on
/// `public::router()`, since a distributed/rotating-IP attacker is not
/// slowed by a per-IP limit. Rejecting malformed tokens before the map
/// insert closes this: only strings shaped like a real token (UUID or
/// 32-char lowercase hex — see `HeartbeatToken::new`) can ever occupy a
/// slot, and the number of distinct VALID tokens is bounded by how many
/// heartbeat monitors actually exist.
pub async fn record_heartbeat(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let token = HeartbeatToken::new(&token).map_err(|_| StatusCode::NOT_FOUND)?;

    if !check_and_record_rate_limit(&state.heartbeat_rate_limiter, token.as_str()).await {
        warn!("heartbeat rate limit exceeded for a token");
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let monitor = match state
        .monitor_repository
        .find_by_heartbeat_token(token.as_str())
        .await
    {
        Ok(Some(monitor)) => monitor,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(err) => {
            warn!(error = %err, "failed to look up monitor by heartbeat token");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // BILLING POLICY: a heartbeat is a user-triggered passive monitoring
    // operation, so PAYG meters the accepted heartbeat receipt itself. The
    // sweeper's later timeout evaluation is platform-side bookkeeping and
    // is intentionally NOT billed.
    let now = Utc::now();
    let request_id = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= MAX_HEARTBEAT_IDEMPOTENCY_KEY_BYTES
                && value.is_ascii()
                && value.bytes().all(|b| b.is_ascii_graphic())
        })
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            format!(
                "heartbeat:{}:{}",
                monitor.id.0,
                now.timestamp_nanos_opt().unwrap_or_default()
            )
        });

    if let Some(metering) = &state.payg_metering {
        match metering
            .execute(
                MeterUsageInput {
                    tenant_id: monitor.tenant_id.0,
                    operation: domain::PaygOperation::HeartbeatProcessing.as_str().to_owned(),
                    units: 1,
                    request_id: format!("heartbeat:{}", request_id),
                },
                || {
                    state
                        .monitor_repository
                        .update_last_heartbeat_at(monitor.id, now)
                },
            )
            .await
        {
            Ok(_) => {}
            Err(ChargeForUsageError::InsufficientBalance) => {
                warn!(monitor_id = %monitor.id.0, "heartbeat rejected: insufficient PAYG balance");
                return Err(StatusCode::PAYMENT_REQUIRED);
            }
            Err(err) => {
                warn!(monitor_id = %monitor.id.0, error = %err, "heartbeat metering/record failed");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else if let Err(err) = state
        .monitor_repository
        .update_last_heartbeat_at(monitor.id, now)
        .await
    {
        warn!(monitor_id = %monitor.id.0, error = %err, "failed to record heartbeat");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    info!(monitor_id = %monitor.id.0, "heartbeat recorded");

    Ok((StatusCode::OK, Json(json!({ "status": "ok" }))))
}

/// Fixed-window per-token rate limiter. Returns `true` if this request is
/// allowed (and records it against the token's current window), `false` if
/// the token has already made `HEARTBEAT_RATE_LIMIT_MAX_REQUESTS` requests
/// within the current `HEARTBEAT_RATE_LIMIT_WINDOW`.
///
/// NOTE: the map is never pruned of stale/inactive tokens. This is bounded
/// in practice by the number of Heartbeat monitors that have EVER pinged
/// this server (one entry per distinct token) AND, as of B34, only ever
/// receives entries for well-formed tokens (see `record_heartbeat`'s doc
/// comment) — a small, slowly-growing set compared to raw request volume,
/// acceptable for now, but a candidate for a periodic sweep if it ever
/// becomes a concern.
async fn check_and_record_rate_limit(limiter: &HeartbeatRateLimiter, token: &str) -> bool {
    let mut buckets = limiter.lock().await;
    let now = Instant::now();

    match buckets.get_mut(token) {
        Some((window_started_at, count)) => {
            if now.duration_since(*window_started_at) >= HEARTBEAT_RATE_LIMIT_WINDOW {
                *window_started_at = now;
                *count = 1;
                true
            } else if *count < HEARTBEAT_RATE_LIMIT_MAX_REQUESTS {
                *count += 1;
                true
            } else {
                false
            }
        }
        None => {
            buckets.insert(token.to_string(), (now, 1));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn empty_limiter() -> HeartbeatRateLimiter {
        Arc::new(tokio::sync::Mutex::new(HashMap::new()))
    }

    #[tokio::test]
    async fn allows_requests_up_to_the_limit() {
        let limiter = empty_limiter();

        for _ in 0..HEARTBEAT_RATE_LIMIT_MAX_REQUESTS {
            assert!(check_and_record_rate_limit(&limiter, "token-a").await);
        }
    }

    #[tokio::test]
    async fn rejects_requests_beyond_the_limit_within_the_window() {
        let limiter = empty_limiter();

        for _ in 0..HEARTBEAT_RATE_LIMIT_MAX_REQUESTS {
            assert!(check_and_record_rate_limit(&limiter, "token-b").await);
        }

        assert!(!check_and_record_rate_limit(&limiter, "token-b").await);
    }

    #[tokio::test]
    async fn different_tokens_have_independent_limits() {
        let limiter = empty_limiter();

        for _ in 0..HEARTBEAT_RATE_LIMIT_MAX_REQUESTS {
            assert!(check_and_record_rate_limit(&limiter, "token-c").await);
        }

        assert!(check_and_record_rate_limit(&limiter, "token-d").await);
    }

    #[tokio::test]
    async fn record_heartbeat_rejects_malformed_token_before_touching_rate_limiter() {
        // B34 regression test: a malformed token must never be able to
        // grow the rate-limiter map. We can't easily call the full
        // handler here without a lot of AppState scaffolding (already
        // covered end-to-end in crates/api/tests/heartbeat.rs), so this
        // test asserts the underlying guarantee directly: HeartbeatToken
        // parsing rejects arbitrary garbage, which is the mechanism
        // `record_heartbeat` now relies on to fail closed before any
        // limiter/database call.
        assert!(HeartbeatToken::new("../../../../etc/passwd").is_err());
        assert!(HeartbeatToken::new("<script>alert(1)</script>").is_err());
        assert!(HeartbeatToken::new("").is_err());
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/api/src/handlers/mod.rs")"
cat <<'EOF' > "$PROJECT/crates/api/src/handlers/mod.rs"
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
pub mod one_time_check;
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
pub mod get_incident;
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
pub mod verify_email_otp;
pub mod verify_totp;
pub mod wallet;

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
pub mod payg_pricing;
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
pub use one_time_check::*;
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
pub use get_incident::*;
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
pub use verify_email_otp::*;
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
pub mod billing_finalization;
pub mod billing_types;
pub mod initiate_checkout;
pub mod verify_checkout;
pub use billing_finalization::{cancel_subscription, get_invoice, get_subscription, list_plans};
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
pub mod change_plan;
pub mod invoices;
pub use add_on_billing::{billing_summary, cancel_add_on, create_add_on, verify_add_on};
pub use change_plan::{change_plan, verify_plan_change};
pub use invoices::list_invoices;

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

pub use wallet::*;

pub mod cancel_proxy_check_job;
pub mod list_proxy_check_results;
pub use cancel_proxy_check_job::*;
pub use list_proxy_check_results::*;
EOF

mkdir -p "$(dirname "$PROJECT/crates/api/src/handlers/one_time_check.rs")"
cat <<'EOF' > "$PROJECT/crates/api/src/handlers/one_time_check.rs"
use axum::{extract::{Extension,Path,State},Json};
use serde::Serialize; use uuid::Uuid;
use domain::shared::types::{AuthenticatedUser,Permission};
use crate::{error::ApiError,state::AppState,validation::parse_monitor_id};
#[derive(Debug,Serialize)] pub struct OneTimeCheckResponse{pub id:String,pub request_id:String,pub status:String}
pub async fn create_one_time_check(State(state):State<AppState>,Extension(user):Extension<AuthenticatedUser>,Path(id):Path<String>)->Result<Json<OneTimeCheckResponse>,ApiError>{
 user.require(Permission::ManageBilling).map_err(|e|ApiError::forbidden(e.to_string()))?;
 let monitor_id=parse_monitor_id(&id)?; let tenant_id=user.tenant_context().tenant_id();
 let monitor=state.monitor_repository.find_by_id(monitor_id).await.map_err(|_|ApiError::internal("monitor lookup failed"))?.ok_or_else(||ApiError::not_found("monitor not found"))?;
 if monitor.tenant_id!=tenant_id{return Err(ApiError::not_found("monitor not found"));}
 if monitor.kind==domain::MonitorKind::Heartbeat{return Err(ApiError::bad_request("heartbeat monitors are passive and cannot run a one-time active check"));}
 state.wallet_repository.get_or_create(tenant_id.0,"USD").await.map_err(|_|ApiError::internal("wallet lookup failed"))?;
 if state.wallet_repository.get_balance_summary(tenant_id.0,"USD").await.map_err(|_|ApiError::internal("wallet lookup failed"))?.available_units<=0{return Err(ApiError::payment_required("available PAYG wallet credit is required for a one-time check","PAYG_INSUFFICIENT_BALANCE"));}
 let request_id=format!("one-time-check:{}",Uuid::new_v4());
 let r=state.scheduling_repository.enqueue_one_time_check(tenant_id,monitor_id,user.user_id(),&request_id).await.map_err(|_|ApiError::internal("failed to queue one-time check"))?;
 Ok(Json(OneTimeCheckResponse{id:r.id.to_string(),request_id:r.request_id,status:r.status}))
}
pub async fn get_one_time_check(State(state):State<AppState>,Extension(user):Extension<AuthenticatedUser>,Path((monitor_id,request_id)):Path<(String,String)>)->Result<Json<OneTimeCheckResponse>,ApiError>{
 user.require(Permission::ViewMonitor).map_err(|e|ApiError::forbidden(e.to_string()))?;
 let monitor_id=parse_monitor_id(&monitor_id)?; let request_id=Uuid::parse_str(&request_id).map_err(|_|ApiError::bad_request("invalid one-time check id"))?;
 let r=state.scheduling_repository.get_one_time_check(user.tenant_context().tenant_id(),monitor_id,request_id).await.map_err(|_|ApiError::internal("one-time check lookup failed"))?.ok_or_else(||ApiError::not_found("one-time check not found"))?;
 Ok(Json(OneTimeCheckResponse{id:r.id.to_string(),request_id:r.request_id,status:r.status}))
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/api/src/handlers/update_monitor.rs")"
cat <<'EOF' > "$PROJECT/crates/api/src/handlers/update_monitor.rs"
use axum::{
    Json,
    extract::{Extension, Path, State},
};
use tracing::info;

use application::{commands::UpdateMonitor, use_cases::UpdateMonitorUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::{MonitorResponse, MonitorWriteRequest},
    state::AppState,
    validation::parse_monitor_id,
};

pub async fn update_monitor(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(body): Json<MonitorWriteRequest>,
) -> Result<Json<MonitorResponse>, ApiError> {
    info!("API: update_monitor");

    let monitor_id = parse_monitor_id(&id)?;

    let command = UpdateMonitor {
        id: monitor_id,
        name: body.name,
        url: body.url,
        method: body.method,
        expected_status: body.expected_status,
        interval_seconds: body.interval_seconds,
        timeout_seconds: body.timeout_seconds,
        assertions: body.assertions,
        is_public: body.is_public,
        ssl_check_enabled: body.ssl_check_enabled,
        ssl_expiry_warning_days: body.ssl_expiry_warning_days,
        kind: body.kind,
        tcp_port: body.tcp_port,
        dns_record_type: body.dns_record_type,
        dns_expected_value: body.dns_expected_value,
        heartbeat_interval_seconds: body.heartbeat_interval_seconds,
        heartbeat_grace_period_seconds: body.heartbeat_grace_period_seconds,
    };

    let use_case = UpdateMonitorUseCase::new(
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    )
    .with_wallet_repository(state.wallet_repository.clone());

    let monitor = use_case
        .execute(command, authenticated_user)
        .await
        .map_err(|err| {
            if err
                .downcast_ref::<domain::shared::types::PermissionDenied>()
                .is_some()
            {
                return ApiError::forbidden(err.to_string());
            }

            let msg = err.to_string();
            let is_validation_error = msg.contains("invalid")
                || msg.contains("cannot")
                || msg.contains("must")
                || msg.contains("required")
                || msg.contains("interval")
                || msg.contains("timeout")
                || msg.contains("expected")
                || msg.contains("url")
                || msg.contains("host")
                || msg.contains("dns")
                || msg.contains("heartbeat");

            if is_validation_error {
                ApiError::bad_request(msg)
            } else {
                ApiError::internal_with_source("failed to update monitor", err)
            }
        })?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    Ok(Json(MonitorResponse::from(monitor)))
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/api/src/handlers/wallet.rs")"
cat <<'EOF' > "$PROJECT/crates/api/src/handlers/wallet.rs"
use crate::{error::ApiError, state::AppState};
use application::ports::RefundRequest;
use axum::{
    Json,
    extract::{Extension, Query, State},
};
use chrono::{DateTime, Datelike, Duration, Utc};
use domain::shared::types::AuthenticatedUser;
use domain::shared::types::Permission;
use domain::{WalletSettings, WalletTopUpSession};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct WalletResponse {
    pub wallet_id: String,
    pub currency: String,
    pub balance_units: i64,
    pub reserved_units: i64,
    pub available_units: i64,
    pub status: String,
    pub low_balance_threshold_units: i64,
    pub critical_balance_threshold_units: i64,
}
#[derive(Debug, Serialize)]
pub struct WalletTransactionResponse {
    pub id: String,
    pub transaction_type: String,
    pub amount_units: i64,
    pub balance_after_units: i64,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<u32>,
}
#[derive(Debug, Serialize)]
pub struct WalletNotificationResponse {
    pub id: String,
    pub event_type: String,
    pub message: String,
    pub read_at: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Deserialize)]
pub struct TopUpRequest {
    pub amount_units: i64,
    pub payment_method: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct TopUpResponse {
    pub checkout_url: String,
    pub provider_checkout_id: String,
    pub amount_units: i64,
}
#[derive(Debug, Deserialize)]
pub struct SettingsRequest {
    pub low_balance_threshold_units: i64,
    pub critical_balance_threshold_units: i64,
    pub auto_top_up_enabled: bool,
    pub auto_top_up_threshold_units: i64,
    pub auto_top_up_amount_units: i64,
    pub payment_method: Option<String>,
    pub daily_top_up_limit_units: i64,
    pub monthly_top_up_limit_units: i64,
    pub retry_limit: i32,
}
#[derive(Debug, Deserialize)]
pub struct RefundRequestBody {
    pub provider_payment_id: String,
    pub amount_units: i64,
    pub currency: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct RefundResponse {
    pub provider_refund_id: String,
    pub status: &'static str,
}
#[derive(Debug, Serialize)]
pub struct UsageReportResponse {
    pub current_balance_units: i64,
    pub today_usage_units: i64,
    pub month_usage_units: i64,
    pub month_spend_units: i64,
    pub projected_spend_units: i64,
    pub daily: Vec<application::ports::UsageDailyPoint>,
    pub by_operation: Vec<application::ports::UsageOperationPoint>,
    pub rows: Vec<UsageRow>,
}
#[derive(Debug, Serialize)]
pub struct UsageRow {
    pub timestamp: String,
    pub operation: String,
    pub units: i64,
    pub price_units: i64,
    pub charge_units: i64,
    pub request_id: String,
    pub status: String,
}
#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub operation: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
}

fn status(balance: i64, low: i64, critical: i64) -> String {
    (if balance <= 0 {
        "BLOCKED"
    } else if balance <= critical {
        "CRITICAL"
    } else if balance <= low {
        "WARNING"
    } else {
        "OK"
    })
    .to_owned()
}

pub async fn get_wallet(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<WalletResponse>, ApiError> {
    user.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    let tenant_id = user.tenant_context().tenant_id().0;
    let wallet = state
        .wallet_repository
        .get_or_create(tenant_id, "USD")
        .await
        .map_err(|_| ApiError::internal("wallet unavailable"))?;
    let settings = state
        .wallet_repository
        .get_settings(tenant_id)
        .await
        .map_err(|_| ApiError::internal("wallet settings unavailable"))?;
    let level = status(wallet.balance_units, settings.low_balance_threshold_units, settings.critical_balance_threshold_units);
    let balance = state.wallet_repository.get_balance_summary(tenant_id, "USD").await.map_err(|_| ApiError::internal("wallet unavailable"))?;
    if level != "OK" {
        let msg = match level.as_str() {
            "BLOCKED" => "PAYG is blocked because your wallet balance is zero.",
            "CRITICAL" => "Your PAYG wallet balance is critically low.",
            _ => "Your PAYG wallet balance is low.",
        };
        let dedupe_key = format!("{}:{}", level, Utc::now().date_naive());
        let _ = state
            .wallet_repository
            .record_balance_notification(tenant_id, &level, &dedupe_key, msg)
            .await;
    }
    Ok(Json(WalletResponse {
        wallet_id: wallet.id.to_string(),
        currency: wallet.currency,
        status: level,
        balance_units: balance.balance_units,
        reserved_units: balance.reserved_units,
        available_units: balance.available_units,
        low_balance_threshold_units: settings.low_balance_threshold_units,
        critical_balance_threshold_units: settings.critical_balance_threshold_units,
    }))
}
pub async fn list_wallet_transactions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Vec<WalletTransactionResponse>>, ApiError> {
    user.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    let tenant_id = user.tenant_context().tenant_id().0;
    let rows = state
        .wallet_repository
        .list_transactions(tenant_id, q.limit.unwrap_or(100).min(100))
        .await
        .map_err(|_| ApiError::internal("wallet unavailable"))?;
    Ok(Json(
        rows.into_iter()
            .map(|t| WalletTransactionResponse {
                id: t.id.to_string(),
                transaction_type: t.transaction_type.as_str().to_owned(),
                amount_units: t.amount_units,
                balance_after_units: t.balance_after_units,
                reference_type: t.reference_type,
                reference_id: t.reference_id,
                description: t.description,
                created_at: t.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

pub async fn notifications(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Vec<WalletNotificationResponse>>, ApiError> {
    user.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    let rows = state
        .wallet_repository
        .list_balance_notifications(user.tenant_context().tenant_id().0, q.limit.unwrap_or(50))
        .await
        .map_err(|_| ApiError::internal("wallet notifications unavailable"))?;
    Ok(Json(
        rows.into_iter()
            .map(|n| WalletNotificationResponse {
                id: n.id.to_string(),
                event_type: n.event_type,
                message: n.message,
                read_at: n.read_at.map(|d| d.to_rfc3339()),
                created_at: n.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

pub async fn top_up(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<TopUpRequest>,
) -> Result<Json<TopUpResponse>, ApiError> {
    user.require(Permission::ManageBilling)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    if req.amount_units < 5_000_000 { return Err(ApiError::bad_request("minimum PAYG top-up is $5")); }
    if req.payment_method.as_deref().is_some_and(|v| v.len() > 200) {
        return Err(ApiError::bad_request("payment method id is too long"));
    }
    let tenant_id = user.tenant_context().tenant_id();
    let checkout_idempotency_key = format!("wallet-checkout:{}", Uuid::new_v4());
    let d = state
        .payment_provider
        .create_wallet_top_up(&application::ports::WalletTopUpRequest {
            tenant_id,
            amount_units: req.amount_units,
            currency: "USD".to_owned(),
            idempotency_key: checkout_idempotency_key,
            payment_method: req.payment_method,
        })
        .await
        .map_err(|e| ApiError::internal_with_source("wallet top-up provider error", e))?;

    state
        .wallet_repository
        .create_top_up_session(&WalletTopUpSession {
            id: Uuid::new_v4(),
            tenant_id: tenant_id.0,
            provider: state.payment_provider.provider_name().to_owned(),
            provider_checkout_id: d.provider_checkout_id.clone(),
            provider_payment_id: None,
            amount_units: req.amount_units,
            refunded_amount_units: 0,
            currency: "USD".to_owned(),
            status: WalletTopUpSession::PENDING.to_owned(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await
        .map_err(|_| ApiError::internal("failed to persist wallet top-up session"))?;

    Ok(Json(TopUpResponse {
        checkout_url: d.checkout_url,
        provider_checkout_id: d.provider_checkout_id,
        amount_units: req.amount_units,
    }))
}
pub async fn get_settings(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<WalletSettings>, ApiError> {
    user.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    let mut s = state
        .wallet_repository
        .get_settings(user.tenant_context().tenant_id().0)
        .await
        .map_err(|_| ApiError::internal("wallet settings unavailable"))?;
    s.tenant_id = Uuid::nil();
    Ok(Json(s))
}
pub async fn update_settings(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<SettingsRequest>,
) -> Result<Json<WalletSettings>, ApiError> {
    user.require(Permission::ManageBilling)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    let tenant_id = user.tenant_context().tenant_id().0;
    let s = WalletSettings {
        tenant_id,
        low_balance_threshold_units: req.low_balance_threshold_units,
        critical_balance_threshold_units: req.critical_balance_threshold_units,
        auto_top_up_enabled: req.auto_top_up_enabled,
        auto_top_up_threshold_units: req.auto_top_up_threshold_units,
        auto_top_up_amount_units: req.auto_top_up_amount_units,
        payment_method: req.payment_method,
        daily_top_up_limit_units: req.daily_top_up_limit_units,
        monthly_top_up_limit_units: req.monthly_top_up_limit_units,
        retry_limit: req.retry_limit,
    };
    if s.critical_balance_threshold_units > s.low_balance_threshold_units
        || s.low_balance_threshold_units < 0
        || s.auto_top_up_threshold_units < 0
        || s.auto_top_up_amount_units < 1_000_000
        || s.auto_top_up_amount_units > 1_000_000_000
        || s.daily_top_up_limit_units < s.auto_top_up_amount_units
        || s.monthly_top_up_limit_units < s.auto_top_up_amount_units
        || s.monthly_top_up_limit_units < s.daily_top_up_limit_units
        || !(0..=10).contains(&s.retry_limit)
    {
        return Err(ApiError::bad_request("invalid wallet settings"));
    }
    if s.auto_top_up_enabled && s.payment_method.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::bad_request(
            "payment method is required for auto top-up",
        ));
    }
    Ok(Json(
        state
            .wallet_repository
            .update_settings(&s)
            .await
            .map_err(|_| ApiError::bad_request("invalid wallet settings"))?,
    ))
}
pub async fn request_refund(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<RefundRequestBody>,
) -> Result<Json<RefundResponse>, ApiError> {
    user.require(Permission::ManageBilling)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    if req.amount_units <= 0 || req.amount_units > 1_000_000_000 {
        return Err(ApiError::bad_request("invalid refund amount"));
    }
    let tenant_id = user.tenant_context().tenant_id();
    let currency = req.currency.unwrap_or_else(|| "USD".to_owned());
    let original = state
        .wallet_repository
        .refundable_top_up(tenant_id.0, &req.provider_payment_id, req.amount_units)
        .await
        .map_err(|_| ApiError::internal("payment lookup failed"))?
        .ok_or_else(|| {
            ApiError::bad_request("payment is not refundable or does not belong to this tenant")
        })?;

    if !original.currency.eq_ignore_ascii_case(&currency) {
        return Err(ApiError::bad_request(
            "refund currency does not match original payment",
        ));
    }

    let idempotency_key = format!(
        "refund:{}:{}:{}",
        tenant_id.0, req.provider_payment_id, req.amount_units
    );
    let refund_request = domain::WalletRefundRequest {
        id: Uuid::new_v4(),
        tenant_id: tenant_id.0,
        provider_payment_id: req.provider_payment_id.clone(),
        amount_units: req.amount_units,
        idempotency_key: idempotency_key.clone(),
        provider_refund_id: None,
        status: "PENDING".to_owned(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let existing = state
        .wallet_repository
        .create_refund_request(&refund_request)
        .await
        .map_err(|_| ApiError::internal("failed to create refund request"))?
        .ok_or_else(|| ApiError::conflict("refund request could not be created"))?;

    if let Some(provider_refund_id) = existing.provider_refund_id {
        return Ok(Json(RefundResponse {
            provider_refund_id,
            status: "pending_webhook",
        }));
    }
    if existing.status != "PENDING" {
        return Err(ApiError::conflict(
            "a refund for this payment and amount is already being processed",
        ));
    }

    let d = state
        .payment_provider
        .refund_payment(&RefundRequest {
            tenant_id,
            provider_payment_id: req.provider_payment_id,
            amount_units: req.amount_units,
            currency,
            idempotency_key: idempotency_key.clone(),
        })
        .await
        .map_err(|e| ApiError::internal_with_source("refund provider error", e))?;

    state
        .wallet_repository
        .set_refund_provider_id(tenant_id.0, &idempotency_key, &d.provider_refund_id)
        .await
        .map_err(|_| ApiError::internal("failed to persist refund provider reference"))?;

    Ok(Json(RefundResponse {
        provider_refund_id: d.provider_refund_id,
        status: "pending_webhook",
    }))
}

pub async fn usage_report(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(q): Query<UsageQuery>,
) -> Result<Json<UsageReportResponse>, ApiError> {
    user.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    let to =
        q.to.as_deref()
            .map(|v| v.parse::<DateTime<Utc>>())
            .transpose()
            .map_err(|_| ApiError::bad_request("invalid to date"))?
            .unwrap_or_else(Utc::now);
    let from = q
        .from
        .as_deref()
        .map(|v| v.parse::<DateTime<Utc>>())
        .transpose()
        .map_err(|_| ApiError::bad_request("invalid from date"))?
        .unwrap_or_else(|| to - Duration::days(30));
    if from >= to {
        return Err(ApiError::bad_request("from must be before to"));
    }
    if to - from > Duration::days(366) {
        return Err(ApiError::bad_request(
            "usage date range cannot exceed 366 days",
        ));
    }
    if q.operation.as_deref().is_some_and(|v| v.len() > 100)
        || q.status.as_deref().is_some_and(|v| v.len() > 32)
    {
        return Err(ApiError::bad_request("usage filter is too long"));
    }
    let tenant_id = user.tenant_context().tenant_id().0;
    let report = state
        .usage_event_repository
        .report(
            tenant_id,
            from,
            to,
            q.operation.as_deref(),
            q.status.as_deref(),
            q.limit.unwrap_or(500),
        )
        .await
        .map_err(|_| ApiError::internal("usage report unavailable"))?;
    let wallet = state
        .wallet_repository
        .get_or_create(tenant_id, "USD")
        .await
        .map_err(|_| ApiError::internal("wallet unavailable"))?;
    let now = Utc::now();
    let day_of_month = now.day().max(1) as f64;
    let next_month = if now.month() == 12 {
        chrono::NaiveDate::from_ymd_opt(now.year() + 1, 1, 1).unwrap()
    } else {
        chrono::NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1).unwrap()
    };
    let month_days = (next_month - now.date_naive()).num_days() as f64 + day_of_month;
    let projected = (report.month_spend_units as f64 / day_of_month * month_days).round() as i64;
    Ok(Json(UsageReportResponse {
        current_balance_units: wallet.balance_units,
        today_usage_units: report.today_usage_units,
        month_usage_units: report.month_usage_units,
        month_spend_units: report.month_spend_units,
        projected_spend_units: projected,
        daily: report.daily,
        by_operation: report.by_operation,
        rows: report
            .rows
            .into_iter()
            .map(|e| UsageRow {
                timestamp: e.created_at.to_rfc3339(),
                operation: e.operation,
                units: e.units_billed,
                price_units: e.unit_price,
                charge_units: e.charge_units,
                request_id: e.request_id,
                status: e.status,
            })
            .collect(),
    }))
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/api/src/router.rs")"
cat <<'EOF' > "$PROJECT/crates/api/src/router.rs"
use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware::{from_fn, from_fn_with_state},
    routing::get,
};
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::LatencyUnit;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::{
    auth_state::AuthState,
    metrics::metrics_handler,
    middleware::{
        agent_auth::agent_auth, agent_id_key_extractor::AgentIdKeyExtractor,
        api_key_auth::api_key_auth, custom_domain_routing::custom_domain_routing,
        jwt_auth::jwt_auth, metrics::record_http_metrics, origin_check::require_matching_origin,
    },
    routes::{
        api_flows, api_keys, auth, billing, checks, dashboard, health, help, internal_agents,
        invitations, members, monitors, notifications, pricing, proxy_checks, public,
        rum_dashboard, status_pages, subscription, support, wallet,
    },
    state::AppState,
};

const MAX_REQUEST_BODY_SIZE: usize = 16 * 1024;

fn cors_layer() -> CorsLayer {
    let allowed_origin = crate::config::cors_allowed_origin_from_env();

    CorsLayer::new()
        .allow_origin(AllowOrigin::exact(
            allowed_origin
                .parse()
                .expect("CORS_ALLOWED_ORIGIN must be a valid HeaderValue"),
        ))
        .allow_credentials(true)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ])
}

pub fn build_router(app_state: AppState, auth_state: AuthState) -> Router {
    let health_routes = health::router().with_state(app_state.clone());

    let monitor_routes = monitors::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid api rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let one_time_check_routes = monitors::one_time_check_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(1)
                .burst_size(2)
                .finish()
                .expect("valid one-time check rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let check_routes = checks::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid api rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let dashboard_routes = dashboard::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid dashboard rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let dashboard_manual_check_routes = dashboard::manual_check_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(1)
                .burst_size(2)
                .finish()
                .expect("valid dashboard manual-check rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let notification_routes = notifications::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid api rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let notification_test_routes = notifications::test_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(5)
                .finish()
                .expect("valid notification test rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let api_flow_routes = api_flows::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(3)
                .finish()
                .expect("valid api flow rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let proxy_check_routes = proxy_checks::router()
        .layer(DefaultBodyLimit::max(1_000_000))
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(3)
                .finish()
                .expect("valid proxy check rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let status_page_routes = status_pages::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid status page rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let rum_dashboard_routes = rum_dashboard::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid rum dashboard rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let custom_domain_routes = status_pages::custom_domain_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(5)
                .finish()
                .expect("valid custom domain rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let billing_routes = billing::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(10)
                .finish()
                .expect("valid billing rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let wallet_routes = wallet::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(10)
                .finish()
                .expect("valid wallet rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let subscription_routes = subscription::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(10)
                .finish()
                .expect("valid subscription rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let billing_webhook_routes = billing::webhook_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(10)
                .finish()
                .expect("valid billing webhook rate-limit configuration"),
        ))
        .with_state(app_state.clone());

    let auth_rate_limit = GovernorConfigBuilder::default()
        .per_second(60)
        .burst_size(5)
        .finish()
        .expect("valid auth rate-limit configuration");

    let auth_routes = auth::router()
        .layer(GovernorLayer::new(auth_rate_limit))
        // SECURITY: reject cross-origin login/registration/oauth/
        // password-reset submissions (see origin_check.rs doc comment).
        // Applied as its own middleware, in addition to (not instead of)
        // `cors_layer()`'s exact-origin allow-list.
        .layer(from_fn(require_matching_origin))
        .with_state(auth_state.clone());

    let totp_verify_rate_limit = GovernorConfigBuilder::default()
        .burst_size(5)
        .per_second(900)
        .finish()
        .expect("valid totp verify rate-limit configuration");

    let totp_verify_routes = auth::totp_verify_router()
        .layer(GovernorLayer::new(totp_verify_rate_limit))
        // SECURITY: same Origin check as `auth_routes` above — this is
        // the unauthenticated leg of a 2FA login (pending_token + code),
        // not yet behind `jwt_auth`.
        .layer(from_fn(require_matching_origin))
        .with_state(auth_state.clone());

    let totp_protected_routes = auth::protected_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid totp management rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    let api_key_routes = api_keys::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid api key rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let membership_routes = members::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid membership rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    let invitation_routes = invitations::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid invitation rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    // NOTE: Scope enforcement now lives inside `api_keys::external_router()`
    // (per-route `require_api_key_scope`), not here. We only authenticate
    // via `api_key_auth` once for the whole external group.
    let external_routes = api_keys::external_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid external api-key rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(app_state.clone(), api_key_auth))
        .with_state(app_state.clone());

    let public_routes = public::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(60)
                .finish()
                .expect("valid public rate-limit configuration"),
        ))
        .with_state(app_state.clone());

    // H10 (SECURITY GAP FIX): `pricing_routes` previously had no rate
    // limiting at all, unlike every other route group in this file
    // (including `public_routes` right above, which is the closest analog
    // since both groups are fully anonymous — no `jwt_auth`/`api_key_auth`
    // `route_layer` is attached to either). `pricing::router()` currently
    // exposes only `POST /pricing/quote`, which is unauthenticated and was
    // therefore floodable without limit: an attacker could hammer it to
    // degrade the DB/CPU cost of quote calculation, or scrape pricing
    // config values at will.
    //
    // Per the original spec this endpoint should eventually distinguish
    // Anonymous (30 req/min) vs Authenticated (120 req/min) callers, but
    // `pricing_routes` has no auth middleware wired at all today — there is
    // no `AuthenticatedUser`/`ApiKeyPrincipal` in scope by the time this
    // layer runs, so the limiter cannot key on "authenticated vs not".
    // Splitting that would require `jwt_auth`/`api_key_auth` to run BEFORE
    // rate limiting and expose auth state to a custom `KeyExtractor` (see
    // `AgentIdKeyExtractor` for the existing precedent) — a real design
    // decision (does the quote endpoint become auth-required at all?
    // optional auth?) that is out of scope for this fix and not decidable
    // from the existing code/ADRs. Until that's decided, every caller is
    // treated as anonymous and gets the single, conservative 30 req/min
    // limit called for in the "Anonymous" tier of the original spec.
    //
    // `per_second(2)` + `burst_size(30)` == an initial burst of 30
    // requests, replenishing at 1 token / 2s thereafter == 30 req/min
    // sustained, matching that Anonymous figure exactly.
    let pricing_routes = pricing::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(30)
                .finish()
                .expect("valid pricing rate-limit configuration"),
        ))
        .with_state(app_state.clone());

    // Admin pricing routes: protected by JWT only; permission check is done
    // inside handlers (require ManageBilling, available to Owner only).
    let admin_pricing_routes = pricing::admin_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid admin pricing rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let help_routes = help::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(60)
                .finish()
                .expect("valid help rate-limit configuration"),
        ))
        .with_state(app_state.clone());

    let support_routes = support::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid support rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    Router::new()
        .route("/metrics", get(metrics_handler))
        .merge(health_routes)
        .nest("/api/v1", monitor_routes)
        .nest("/api/v1", one_time_check_routes)
        .nest("/api/v1", check_routes)
        .nest("/api/v1", dashboard_routes)
        .nest("/api/v1", dashboard_manual_check_routes)
        .nest("/api/v1", notification_routes)
        .nest("/api/v1", notification_test_routes)
        .nest("/api/v1", api_flow_routes)
        .nest("/api/v1", proxy_check_routes)
        .nest("/api/v1", status_page_routes)
        .nest("/api/v1", rum_dashboard_routes)
        .nest("/api/v1", custom_domain_routes)
        .nest("/api/v1", billing_webhook_routes)
        .nest("/api/v1", billing_routes)
        .nest("/api/v1", subscription_routes)
        .nest("/api/v1", wallet_routes)
        .nest("/api/v1", pricing_routes)
        .nest("/api/v1", help_routes)
        .nest("/api/v1", support_routes)
        .nest("/api/v1/admin", admin_pricing_routes)
        .nest("/api/v1", api_key_routes)
        .nest("/api/v1", membership_routes)
        .nest("/api/v1", invitation_routes)
        .nest("/api/v1", external_routes)
        .nest("/api/v1/auth", auth_routes)
        .nest("/api/v1/auth", totp_verify_routes)
        .nest("/api/v1/auth", totp_protected_routes)
        .nest("/api/v1/public", public_routes)
        .layer(from_fn(record_http_metrics))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                ),
        )
        .layer(cors_layer())
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_SIZE))
        .layer(from_fn_with_state(app_state.clone(), custom_domain_routing))
}

pub fn build_internal_agent_router(app_state: AppState) -> Router {
    let internal_agent_routes = internal_agents::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .key_extractor(AgentIdKeyExtractor)
                .per_millisecond(20)
                .burst_size(250)
                .finish()
                .expect("valid internal agent rate-limit configuration"),
        ))
        .route_layer(axum::middleware::from_fn(agent_auth))
        .with_state(app_state.clone());

    Router::new()
        .nest("/internal/v1", internal_agent_routes)
        .layer(from_fn(record_http_metrics))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                ),
        )
}

pub fn build_test_router(app_state: AppState, auth_state: AuthState) -> Router {
    let health_routes = health::router().with_state(app_state.clone());

    let monitor_routes = monitors::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let check_routes = checks::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let dashboard_routes = dashboard::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let dashboard_manual_check_routes = dashboard::manual_check_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let notification_routes = notifications::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let notification_test_routes = notifications::test_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let api_flow_routes = api_flows::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let proxy_check_routes = proxy_checks::router()
        .layer(DefaultBodyLimit::max(1_000_000))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let status_page_routes = status_pages::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let rum_dashboard_routes = rum_dashboard::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let custom_domain_routes = status_pages::custom_domain_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let billing_routes = billing::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let subscription_routes = subscription::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let billing_webhook_routes = billing::webhook_router().with_state(app_state.clone());

    let api_key_routes = api_keys::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let membership_routes = members::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    let invitation_routes = invitations::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    // Same as in `build_router`: scope enforcement is inside
    // `external_router()`. Only authentication is layered here.
    let external_routes = api_keys::external_router()
        .route_layer(from_fn_with_state(app_state.clone(), api_key_auth))
        .with_state(app_state.clone());

    let auth_routes = auth::router().with_state(auth_state.clone());

    let totp_verify_routes = auth::totp_verify_router().with_state(auth_state.clone());

    let totp_protected_routes = auth::protected_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    let public_routes = public::router().with_state(app_state.clone());

    let internal_agent_routes = internal_agents::router()
        .route_layer(axum::middleware::from_fn(agent_auth))
        .with_state(app_state.clone());

    // NOTE (H10): deliberately still NO `GovernorLayer` here, consistent
    // with every other route in `build_test_router` — this test router
    // exists precisely so the E2E suite ISN'T subject to IP-based rate
    // limiting (which would make unrelated tests flaky/order-dependent
    // under `oneshot()`). Isolated rate-limit coverage for pricing lives in
    // `crates/api/tests/pricing_rate_limit.rs`, which uses the new
    // `build_pricing_only_router_with_rate_limit` below instead of this
    // function — mirroring how `build_auth_only_router_with_rate_limit`
    // already does the same for `auth_routes`.
    let pricing_routes = pricing::router().with_state(app_state.clone());

    let help_routes = help::router().with_state(app_state.clone());

    let support_routes = support::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let one_time_check_routes = monitors::one_time_check_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Admin pricing routes for test router: JWT only, permission check in handlers.
    let admin_pricing_routes = pricing::admin_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    Router::new()
        .route("/metrics", get(metrics_handler))
        .merge(health_routes)
        .nest("/api/v1", monitor_routes)
        .nest("/api/v1", one_time_check_routes)
        .nest("/api/v1", check_routes)
        .nest("/api/v1", dashboard_routes)
        .nest("/api/v1", dashboard_manual_check_routes)
        .nest("/api/v1", notification_routes)
        .nest("/api/v1", notification_test_routes)
        .nest("/api/v1", api_flow_routes)
        .nest("/api/v1", proxy_check_routes)
        .nest("/api/v1", status_page_routes)
        .nest("/api/v1", rum_dashboard_routes)
        .nest("/api/v1", custom_domain_routes)
        .nest("/api/v1", billing_webhook_routes)
        .nest("/api/v1", billing_routes)
        .nest("/api/v1", subscription_routes)
        .nest("/api/v1", pricing_routes)
        .nest("/api/v1", help_routes)
        .nest("/api/v1", support_routes)
        .nest("/api/v1/admin", admin_pricing_routes)
        .nest("/api/v1", api_key_routes)
        .nest("/api/v1", membership_routes)
        .nest("/api/v1", invitation_routes)
        .nest("/api/v1", external_routes)
        .nest("/api/v1/auth", auth_routes)
        .nest("/api/v1/auth", totp_verify_routes)
        .nest("/api/v1/auth", totp_protected_routes)
        .nest("/api/v1/public", public_routes)
        .nest("/internal/v1", internal_agent_routes)
        .layer(from_fn(record_http_metrics))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                ),
        )
        .layer(cors_layer())
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_SIZE))
        .layer(from_fn_with_state(app_state.clone(), custom_domain_routing))
}

pub fn build_auth_only_router_with_rate_limit(auth_state: AuthState) -> Router {
    let auth_rate_limit = GovernorConfigBuilder::default()
        .per_second(60)
        .burst_size(5)
        .finish()
        .expect("valid auth rate-limit configuration");

    let auth_routes = auth::router()
        .layer(GovernorLayer::new(auth_rate_limit))
        .with_state(auth_state);

    Router::new().nest("/api/v1/auth", auth_routes)
}

/// H10: minimal router exposing ONLY `pricing_routes`, with the same
/// production `GovernorLayer` wired onto it in `build_router` above,
/// for isolated E2E rate-limit testing.
///
/// Mirrors `build_auth_only_router_with_rate_limit` immediately above it
/// (same shape, same reason for existing): `build_test_router` deliberately
/// omits `GovernorLayer` from every route group so the rest of the E2E
/// suite isn't flaky/order-dependent under IP-based rate limiting, which
/// means that shared harness structurally cannot be used to verify this
/// behavior. This constructor exists solely so
/// `crates/api/tests/pricing_rate_limit.rs` can exercise the real limiter
/// end-to-end without affecting any other test.
pub fn build_pricing_only_router_with_rate_limit(app_state: AppState) -> Router {
    let pricing_routes = pricing::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(30)
                .finish()
                .expect("valid pricing rate-limit configuration"),
        ))
        .with_state(app_state);

    Router::new().nest("/api/v1", pricing_routes)
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/api/src/routes/monitors.rs")"
cat <<'EOF' > "$PROJECT/crates/api/src/routes/monitors.rs"
use axum::{
    Router,
    routing::{get, post, put},
};

use crate::{
    handlers::{
        acknowledge_incident,
        advanced_monitoring::{
            list_escalation_policies, list_maintenance_windows, update_escalation_policy,
        },
        create_escalation_policy, create_maintenance_window, create_monitor, delete_monitor,
        disable_monitor, enable_monitor, get_incident, get_monitor, list_incidents,
        list_monitor_checks, list_monitors, update_monitor, create_one_time_check, get_one_time_check,
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
        .route(
            "/escalation-policies",
            post(create_escalation_policy).get(list_escalation_policies),
        )
        .route("/escalation-policies/{id}", put(update_escalation_policy))
        .route("/incidents/{incident_id}", get(get_incident))
        .route(
            "/incidents/{incident_id}/acknowledge",
            post(acknowledge_incident),
        )
}


pub fn one_time_check_router() -> Router<AppState> {
    Router::new()
        .route("/monitors/{id}/checks/one-time", post(create_one_time_check))
        .route("/monitors/{id}/checks/one-time/{request_id}", get(get_one_time_check))
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/api/src/state.rs")"
cat <<'EOF' > "$PROJECT/crates/api/src/state.rs"
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use application::ports::{
    AddOnSubscriptionRepository, AgentLeaseRepository, ApiFlowRepository, ApiKeyRepository,
    AuditLogEntryRepository, BillingAuditLogRepository, BillingWebhookIdempotencyRepository,
    CheckRepository, CheckoutDetails, CheckoutRequest, CheckoutSessionRepository,
    CustomDomainVerifier, DashboardRepository, DiagnosticReportRepository,
    EscalationPolicyRepository, FinancialAuditLogRepository, GeoIpLookup, HealthCheckPort,
    HealthCheckResult, HelpArticleRepository, HelpCategoryRepository, IncidentRepository,
    LogoStorage, MaintenanceWindowRepository, MonitorRepository, NotificationChannelRepository,
    NotificationService, PAYGPricingRepository, PasswordHasher, PaymentProvider, PaymentRepository,
    PaymentVerification, PricingConfigRepository, ProxyCheckJobRepository, RateLimiter,
    RateLimiterResult, RumAnalyticsRepository, RumEventRepository, RumSiteRepository,
    SchedulingRepository, SecretCipher, SmsProviderPricing, StatusPageCache, StatusPageRepository,
    SubscriptionRepository, SubscriptionUsagePeriodRepository, SubscriptionUsageTransaction,
    SupportTicketRepository, UsageEventRepository, WalletRepository,
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

use application::dto::{DashboardBadges, DashboardOverview};

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
    pub scheduling_repository: Arc<dyn SchedulingRepository>,
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
    pub wallet_repository: Arc<dyn WalletRepository>,
    pub payg_pricing_repository: Arc<dyn PAYGPricingRepository>,
    pub usage_event_repository: Arc<dyn UsageEventRepository>,
    pub subscription_plan_change_repository:
        Arc<dyn application::ports::SubscriptionPlanChangeRepository>,
    pub invoice_repository: Arc<dyn application::ports::InvoiceRepository>,
    pub payment_repository: Arc<dyn PaymentRepository>,
    pub financial_audit_log_repository: Arc<dyn FinancialAuditLogRepository>,
    /// Shared PAYG metering service. Optional so lightweight test/development
    /// AppState constructors can remain usable without a wallet backend.
    pub payg_metering: Option<Arc<application::MeterUsageUseCase>>,
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
            performance: application::dto::DashboardPerformance {
                uptime_percentage: None,
                avg_latency_ms: None,
                trend: Vec::new(),
            },
        })
    }

    async fn get_badges(&self, _tenant_id: TenantId) -> RepoResult<DashboardBadges> {
        Ok(DashboardBadges {
            active_incidents: 0,
            unread_notifications: 0,
            pending_invitations: 0,
        })
    }
}

#[derive(Clone, Default)]
struct NoopSchedulingRepository;

#[async_trait]
impl SchedulingRepository for NoopSchedulingRepository {
    async fn claim_due_monitors(
        &self,
        _limit: u32,
        _lease_owner: uuid::Uuid,
        _lease_duration: std::time::Duration,
        _now: DateTime<Utc>,
    ) -> RepoResult<Vec<application::ports::ClaimedMonitor>> {
        Ok(Vec::new())
    }

    async fn complete_claim(
        &self,
        _monitor_id: MonitorId,
        _lease_owner: uuid::Uuid,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn request_immediate_checks(&self, _tenant_id: TenantId) -> RepoResult<u64> {
        Ok(0)
    }

    async fn recover_expired_leases(&self, _now: DateTime<Utc>) -> RepoResult<u64> { Ok(0) }
    async fn enqueue_one_time_check(&self,_:TenantId,_:MonitorId,_:domain::shared::ids::UserId,_:&str)->RepoResult<domain::OneTimeCheckRequest>{Err("scheduling repository is not configured".into())}
    async fn claim_one_time_checks(&self,_:u32,_:uuid::Uuid,_:std::time::Duration,_:DateTime<Utc>)->RepoResult<Vec<application::ports::ClaimedOneTimeCheck>>{Ok(vec![])}
    async fn complete_one_time_check(&self,_:uuid::Uuid,_:uuid::Uuid,_:&str,_:Option<&str>)->RepoResult<()> {Ok(())}
    async fn get_one_time_check(&self,_:TenantId,_:MonitorId,_:uuid::Uuid)->RepoResult<Option<domain::OneTimeCheckRequest>>{Ok(None)}
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
struct NoopSubscriptionPlanChangeRepository;
#[async_trait]
impl application::ports::SubscriptionPlanChangeRepository for NoopSubscriptionPlanChangeRepository {
    async fn create(&self, _: &domain::SubscriptionPlanChange) -> RepoResult<()> {
        Ok(())
    }
    async fn update(&self, _: &domain::SubscriptionPlanChange) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: uuid::Uuid,
        _: uuid::Uuid,
    ) -> RepoResult<Option<domain::SubscriptionPlanChange>> {
        Ok(None)
    }
}
#[derive(Clone, Default)]
struct NoopPaymentRepository;
#[async_trait]
impl PaymentRepository for NoopPaymentRepository {
    async fn record(&self, _p: &domain::Payment) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_provider_payment_id(
        &self,
        _provider: &str,
        _id: &str,
    ) -> RepoResult<Option<domain::Payment>> {
        Ok(None)
    }
}

struct NoopFinancialAuditLogRepository;

#[async_trait]
impl FinancialAuditLogRepository for NoopFinancialAuditLogRepository {
    async fn create(&self, _entry: &domain::FinancialAuditLogEntry) -> RepoResult<()> {
        Ok(())
    }
    async fn list_by_tenant(
        &self,
        _tenant_id: uuid::Uuid,
    ) -> RepoResult<Vec<domain::FinancialAuditLogEntry>> {
        Ok(Vec::new())
    }
}

struct NoopInvoiceRepository;
#[async_trait]
impl application::ports::InvoiceRepository for NoopInvoiceRepository {
    async fn create_invoice(
        &self,
        _: &domain::Invoice,
        _: &[domain::InvoiceLine],
    ) -> RepoResult<()> {
        Ok(())
    }
    async fn list_by_tenant(&self, _: uuid::Uuid, _: u32) -> RepoResult<Vec<domain::Invoice>> {
        Ok(vec![])
    }
    async fn find_by_id_for_tenant(
        &self,
        _: uuid::Uuid,
        _: uuid::Uuid,
    ) -> RepoResult<Option<domain::Invoice>> {
        Ok(None)
    }
    async fn update_status_for_tenant(
        &self,
        _: uuid::Uuid,
        _: uuid::Uuid,
        _: domain::InvoiceStatus,
        _: Option<chrono::DateTime<chrono::Utc>>,
    ) -> RepoResult<()> {
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
        _expected_old_plan: Option<domain::enums::PlanTier>,
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
    async fn update(&self, _: &domain::AddOnSubscription) -> RepoResult<()> {
        Ok(())
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

#[derive(Clone, Default)]
struct NoopWalletRepository;

#[async_trait]
impl WalletRepository for NoopWalletRepository {
    async fn get_balance_summary(&self,_tenant_id:uuid::Uuid,_currency:&str)->RepoResult<domain::WalletBalanceSummary>{Ok(domain::WalletBalanceSummary{balance_units:0,reserved_units:0,available_units:0})}
    async fn get_or_create(
        &self,
        tenant_id: uuid::Uuid,
        currency: &str,
    ) -> RepoResult<domain::Wallet> {
        Ok(domain::Wallet::new(
            uuid::Uuid::new_v4(),
            tenant_id,
            currency,
        ))
    }
    async fn list_transactions(
        &self,
        _tenant_id: uuid::Uuid,
        _limit: u32,
    ) -> RepoResult<Vec<domain::WalletTransaction>> {
        Ok(vec![])
    }
    async fn reserve(
        &self,
        _tenant_id: uuid::Uuid,
        _amount_units: i64,
        _operation: &str,
        _request_id: &str,
        _expires_at: DateTime<Utc>,
    ) -> RepoResult<application::ports::ReservationOutcome> {
        Err("wallet repository is not configured".into())
    }
    async fn capture(
        &self,
        _reservation_id: uuid::Uuid,
        _usage_event: Option<&domain::UsageEvent>,
        _audit_entries: &[domain::FinancialAuditLogEntry],
    ) -> RepoResult<domain::WalletTransaction> {
        Err("wallet repository is not configured".into())
    }
    async fn release(&self, _reservation_id: uuid::Uuid) -> RepoResult<()> {
        Err("wallet repository is not configured".into())
    }
    async fn credit(
        &self,
        _tenant_id: uuid::Uuid,
        _amount_units: i64,
        _transaction_type: domain::WalletTransactionType,
        _idempotency_key: &str,
        _description: Option<&str>,
    ) -> RepoResult<domain::WalletTransaction> {
        Err("wallet repository is not configured".into())
    }
    async fn create_top_up_session(&self, _session: &domain::WalletTopUpSession) -> RepoResult<()> {
        Err("wallet repository is not configured".into())
    }
    async fn find_top_up_session(
        &self,
        _tenant_id: uuid::Uuid,
        _provider_checkout_id: &str,
    ) -> RepoResult<Option<domain::WalletTopUpSession>> {
        Ok(None)
    }
    async fn complete_top_up_session(
        &self,
        _tenant_id: uuid::Uuid,
        _provider_checkout_id: &str,
        _provider_payment_id: &str,
    ) -> RepoResult<Option<domain::WalletTopUpSession>> {
        Ok(None)
    }
    async fn refundable_top_up(
        &self,
        _tenant_id: uuid::Uuid,
        _provider_payment_id: &str,
        _amount_units: i64,
    ) -> RepoResult<Option<domain::WalletTopUpSession>> {
        Ok(None)
    }
    async fn record_verified_refund(
        &self,
        _tenant_id: uuid::Uuid,
        _provider_payment_id: &str,
        _amount_units: i64,
        _provider_refund_id: &str,
    ) -> RepoResult<bool> {
        Ok(false)
    }
    async fn record_auto_top_up_attempt(
        &self,
        _attempt: &domain::AutoTopUpAttempt,
    ) -> RepoResult<()> {
        Ok(())
    }
    async fn create_refund_request(
        &self,
        _request: &domain::WalletRefundRequest,
    ) -> RepoResult<Option<domain::WalletRefundRequest>> {
        Ok(None)
    }
    async fn set_refund_provider_id(
        &self,
        _tenant_id: uuid::Uuid,
        _idempotency_key: &str,
        _provider_refund_id: &str,
    ) -> RepoResult<()> {
        Ok(())
    }
    async fn claim_auto_top_up(
        &self,
        _tenant_id: uuid::Uuid,
        _lease_id: uuid::Uuid,
        _lease_for: std::time::Duration,
    ) -> RepoResult<bool> {
        Ok(false)
    }
    async fn release_auto_top_up(
        &self,
        _tenant_id: uuid::Uuid,
        _lease_id: uuid::Uuid,
    ) -> RepoResult<()> {
        Ok(())
    }
    async fn get_settings(&self, tenant_id: uuid::Uuid) -> RepoResult<domain::WalletSettings> {
        let s = domain::WalletSettings {
            tenant_id,
            ..domain::WalletSettings::default()
        };
        Ok(s)
    }
    async fn update_settings(
        &self,
        settings: &domain::WalletSettings,
    ) -> RepoResult<domain::WalletSettings> {
        Ok(settings.clone())
    }
    async fn expire_credit(
        &self,
        _tenant_id: uuid::Uuid,
        _amount_units: i64,
        _idempotency_key: &str,
        _description: Option<&str>,
    ) -> RepoResult<Option<domain::WalletTransaction>> {
        Ok(None)
    }
    async fn grant_promotional_credit(
        &self,
        _tenant_id: uuid::Uuid,
        _amount_units: i64,
        _expires_at: DateTime<Utc>,
        _idempotency_key: &str,
        _description: Option<&str>,
    ) -> RepoResult<domain::WalletTransaction> {
        Err("wallet repository is not configured".into())
    }
    async fn expire_promotional_credits(&self, _limit: u32) -> RepoResult<u64> {
        Ok(0)
    }
    async fn expire_pending_reservations(&self, _limit: u32) -> RepoResult<u64> {
        Ok(0)
    }
    async fn auto_top_up_candidates(
        &self,
        _limit: u32,
    ) -> RepoResult<Vec<domain::AutoTopUpCandidate>> {
        Ok(vec![])
    }
    async fn record_balance_notification(
        &self,
        _tenant_id: uuid::Uuid,
        _event_type: &str,
        _dedupe_key: &str,
        _message: &str,
    ) -> RepoResult<bool> {
        Ok(false)
    }
    async fn list_balance_notifications(
        &self,
        _tenant_id: uuid::Uuid,
        _limit: u32,
    ) -> RepoResult<Vec<domain::WalletNotification>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopPAYGPricingRepository;
#[async_trait]
impl PAYGPricingRepository for NoopPAYGPricingRepository {
    async fn find_active(&self, _operation: &str) -> RepoResult<Option<domain::PAYGPricing>> {
        Ok(None)
    }
    async fn list_active(&self) -> RepoResult<Vec<domain::PAYGPricing>> {
        Ok(Vec::new())
    }
}

#[derive(Clone, Default)]
struct NoopUsageEventRepository;
#[async_trait]
impl UsageEventRepository for NoopUsageEventRepository {
    async fn find_by_request(
        &self,
        _tenant_id: uuid::Uuid,
        _request_id: &str,
    ) -> RepoResult<Option<domain::UsageEvent>> {
        Ok(None)
    }
    async fn create(&self, event: &domain::UsageEvent) -> RepoResult<domain::UsageEvent> {
        Ok(event.clone())
    }
    async fn report(
        &self,
        _tenant_id: uuid::Uuid,
        _from: DateTime<Utc>,
        _to: DateTime<Utc>,
        _operation: Option<&str>,
        _status: Option<&str>,
        _limit: u32,
    ) -> RepoResult<application::ports::UsageReport> {
        Ok(application::ports::UsageReport {
            today_usage_units: 0,
            month_usage_units: 0,
            month_spend_units: 0,
            daily: vec![],
            by_operation: vec![],
            rows: vec![],
        })
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
            scheduling_repository: Arc::new(NoopSchedulingRepository),
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
            wallet_repository: Arc::new(NoopWalletRepository),
            payg_pricing_repository: Arc::new(NoopPAYGPricingRepository),
            usage_event_repository: Arc::new(NoopUsageEventRepository),
            subscription_plan_change_repository: Arc::new(NoopSubscriptionPlanChangeRepository),
            invoice_repository: Arc::new(NoopInvoiceRepository),
            financial_audit_log_repository: Arc::new(NoopFinancialAuditLogRepository),
            payment_repository: Arc::new(NoopPaymentRepository),
            payg_metering: None,
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
            invoice_repository: Arc::new(NoopInvoiceRepository),
            financial_audit_log_repository: Arc::new(NoopFinancialAuditLogRepository),
            payment_repository: Arc::new(NoopPaymentRepository),
            subscription_plan_change_repository: Arc::new(NoopSubscriptionPlanChangeRepository),
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
            scheduling_repository: Arc::new(NoopSchedulingRepository),
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
            wallet_repository: Arc::new(NoopWalletRepository),
            payg_pricing_repository: Arc::new(NoopPAYGPricingRepository),
            usage_event_repository: Arc::new(NoopUsageEventRepository),
            payg_metering: None,
        }
    }

    pub fn with_payg_metering(mut self, metering: Arc<application::MeterUsageUseCase>) -> Self {
        self.payg_metering = Some(metering);
        self
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

    pub fn with_scheduling_repository<R>(mut self, scheduling_repository: R) -> Self
    where
        R: SchedulingRepository + 'static,
    {
        self.scheduling_repository = Arc::new(scheduling_repository);
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

    pub fn with_wallet_repository<R>(mut self, repository: R) -> Self
    where
        R: WalletRepository + 'static,
    {
        self.wallet_repository = Arc::new(repository);
        self
    }

    pub fn with_payg_pricing_repository<R>(mut self, repository: R) -> Self
    where
        R: PAYGPricingRepository + 'static,
    {
        self.payg_pricing_repository = Arc::new(repository);
        self
    }

    pub fn with_usage_event_repository<R>(mut self, repository: R) -> Self
    where
        R: UsageEventRepository + 'static,
    {
        self.usage_event_repository = Arc::new(repository);
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
    pub fn with_subscription_plan_change_repository<R>(mut self, repo: R) -> Self
    where
        R: application::ports::SubscriptionPlanChangeRepository + 'static,
    {
        self.subscription_plan_change_repository = Arc::new(repo);
        self
    }
    pub fn with_payment_repository<R>(mut self, repo: R) -> Self
    where
        R: PaymentRepository + 'static,
    {
        self.payment_repository = Arc::new(repo);
        self
    }

    pub fn with_financial_audit_log_repository<R>(mut self, repo: R) -> Self
    where
        R: FinancialAuditLogRepository + 'static,
    {
        self.financial_audit_log_repository = Arc::new(repo);
        self
    }

    pub fn with_invoice_repository<R>(mut self, repo: R) -> Self
    where
        R: application::ports::InvoiceRepository + 'static,
    {
        self.invoice_repository = Arc::new(repo);
        self
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/application/src/ports/mod.rs")"
cat <<'EOF' > "$PROJECT/crates/application/src/ports/mod.rs"
pub mod api_flow_repository;
pub mod check_repository;
pub mod diagnostic_report_repository;
pub mod http_client;
pub mod id_generator;
pub mod incident_repository;
pub mod monitor_repository;
pub mod notification_channel_repository;
pub mod notification_outbox_repository;
pub mod notification_outbox_sender;
pub mod notification_service;
pub mod password_hasher;
pub mod proxy_check_job_repository;
pub mod proxy_credential_cipher;
pub mod secret_cipher;
pub mod subscriber_repository;
pub mod token_service;
pub mod user_repository;

// Sabbin ports:
pub mod api_key_repository;
pub mod audit_log_entry_repository;
pub mod backup_code_repository;
pub mod custom_domain_verifier;
pub mod escalation_policy_repository;
pub mod financial_audit_log_repository;
pub mod maintenance_window_repository;
pub mod pending_auth_cache;
pub mod rate_limiter;
pub mod registration_transaction;
pub mod status_page_cache;
pub mod status_page_repository;
pub mod tenant_membership_repository;
pub mod tenant_repository;

// B5 — Email verification:
pub mod email_verification_mailer;
pub mod email_verification_token_repository;

// OAuth — Continue with Google / Apple:
pub mod id_token_verifier;

// B9 — Tenant invitations:
pub mod tenant_invitation_repository;

// B34 — Public Endpoint Security & Efficiency:
pub mod health_check;

// H6 — Dashboard Overview:
pub mod dashboard_repository;

// H14 — Help Center & Support:
pub mod help_article_repository;
pub mod help_category_repository;
pub mod support_ticket_repository;

pub use api_flow_repository::ApiFlowRepository;
pub use check_repository::CheckRepository;
pub use diagnostic_report_repository::DiagnosticReportRepository;
pub use http_client::*;
pub use id_generator::*;
pub use incident_repository::IncidentRepository;
// B13 — Atomic Quota Enforcement: re-export QuotaCheckOutcome alongside
// MonitorRepository so callers can `use application::ports::{MonitorRepository, QuotaCheckOutcome};`
pub use monitor_repository::{MonitorRepository, QuotaCheckOutcome};
pub use notification_channel_repository::NotificationChannelRepository;
pub use notification_outbox_repository::NotificationOutboxRepository;
pub use notification_outbox_sender::NotificationOutboxSender;
pub use notification_service::{NotificationService, NotifyResult};
pub use password_hasher::PasswordHasher;
pub use proxy_check_job_repository::{ProxyCheckJobRepository, ProxyResultStatusFilter};
pub use proxy_credential_cipher::{
    EncryptedProxyCredentials, ProxyCredentialCipher, ProxyCredentialCipherError,
};
pub use secret_cipher::{SecretCipher, SecretCipherError};
pub use subscriber_repository::SubscriberRepository;
pub use token_service::{TokenService, VerifiedToken};
pub use user_repository::UserRepository;

// Sabbin re-exports:
pub use api_key_repository::ApiKeyRepository;
pub use audit_log_entry_repository::AuditLogEntryRepository;
pub use backup_code_repository::BackupCodeRepository;
pub use custom_domain_verifier::{CustomDomainVerifier, VerifierResult};
pub use escalation_policy_repository::EscalationPolicyRepository;
pub use financial_audit_log_repository::{FinancialAuditLogRepository, FinancialAuditRepoResult};
pub use maintenance_window_repository::MaintenanceWindowRepository;
pub use pending_auth_cache::{PendingAuthCache, PendingAuthCacheResult};
pub use rate_limiter::{RateLimiter, RateLimiterResult};
pub use registration_transaction::{RegistrationTransaction, RegistrationTransactionResult};
// B34 — Public Endpoint Security & Efficiency: PUBLIC_STATUS_LIST_CACHE_KEY
// is the shared cache key for the public status *list* endpoint, used by
// both the API (get_public_status) and the worker (IncidentManager).
pub use status_page_cache::{CacheResult, PUBLIC_STATUS_LIST_CACHE_KEY, StatusPageCache};
pub use status_page_repository::StatusPageRepository;
pub use tenant_membership_repository::{LastOwnerGuardOutcome, TenantMembershipRepository};
pub use tenant_repository::TenantRepository;

pub mod logo_storage;
pub use logo_storage::{LogoStorage, StorageResult};

// B5 — Email verification re-exports:
pub use email_verification_mailer::{EmailVerificationMailer, MailerResult};
pub use email_verification_token_repository::EmailVerificationTokenRepository;

// OAuth re-exports:
pub use id_token_verifier::{IdTokenVerifier, OidcVerifierResult, VerifiedIdentity};
pub mod oauth_identity_repository;
pub use oauth_identity_repository::{OAuthIdentityRepoResult, OAuthIdentityRepository};

// B6 — Password reset:
pub mod password_reset_mailer;
pub mod reset_password_token_repository;
pub use password_reset_mailer::{PasswordResetMailer, PasswordResetMailerResult};
pub use reset_password_token_repository::ResetPasswordTokenRepository;

// B9 — Tenant invitation re-export:
pub use tenant_invitation_repository::TenantInvitationRepository;

// B11 — Plan / Subscription / Entitlement model:
pub mod subscription_repository;
pub use subscription_repository::SubscriptionRepository;

// B14 — Billing Provider Abstraction & Checkout:
pub mod checkout_session_repository;
pub mod payment_provider;
pub use checkout_session_repository::CheckoutSessionRepository;
pub use payment_provider::{
    CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentProviderResult, PaymentVerification,
};
pub use payment_provider::{RefundDetails, RefundRequest, WalletTopUpRequest};

// B15 — Billing Webhooks:
pub mod billing_audit_log_repository;
pub mod billing_webhook_idempotency_repository;
pub use billing_audit_log_repository::BillingAuditLogRepository;
pub use billing_webhook_idempotency_repository::BillingWebhookIdempotencyRepository;

// B17 — Durable Scheduler:
pub mod scheduling_repository;
pub use scheduling_repository::{ClaimedMonitor, ClaimedOneTimeCheck, SchedulingRepository};

// B20 — Agent Execution & mTLS:
pub mod agent_lease_repository;
pub use agent_lease_repository::{
    AgentLease, AgentLeaseRepository, RepoResult, SubmitAgentResultOutcome,
};

// B22 — History Retention Enforcement:
pub mod history_retention_repository;
pub use history_retention_repository::HistoryRetentionRepository;

// B34 — Public Endpoint Security & Efficiency:
pub use health_check::{HealthCheckPort, HealthCheckResult};

// H6 — Dashboard Overview:
pub use dashboard_repository::DashboardRepository;
pub mod pricing_config_repository;
pub use pricing_config_repository::PricingConfigRepository;

// H12 — Proration Service:
pub mod subscription_usage_period_repository;
pub mod subscription_usage_transaction;
pub use subscription_usage_period_repository::SubscriptionUsagePeriodRepository;
pub use subscription_usage_transaction::{
    SubscriptionUsageTransaction, SubscriptionUsageTransactionResult,
};

pub mod add_on_subscription_repository;
pub use add_on_subscription_repository::AddOnSubscriptionRepository;

pub mod sms_provider_pricing;
pub use sms_provider_pricing::{SmsPricingResult, SmsProviderPricing};

// H14 — Help Center & Support:
pub use help_article_repository::HelpArticleRepository;
pub use help_category_repository::HelpCategoryRepository;
pub use support_ticket_repository::SupportTicketRepository;

// RUM (Real User Monitoring):
pub mod geo_ip_lookup;
pub mod rum_analytics_repository;
pub mod rum_event_repository;
pub mod rum_site_repository;
pub use geo_ip_lookup::GeoIpLookup;
pub use rum_analytics_repository::RumAnalyticsRepository;
pub use rum_event_repository::RumEventRepository;
pub use rum_site_repository::RumSiteRepository;

pub mod invoice_repository;
pub mod subscription_plan_change_repository;
pub mod wallet_repository;
pub use invoice_repository::{InvoiceRepoResult, InvoiceRepository};
pub use subscription_plan_change_repository::{
    PlanChangeRepoResult, SubscriptionPlanChangeRepository,
};
pub use wallet_repository::{ReservationOutcome, WalletRepoResult, WalletRepository};

pub mod payg_pricing_repository;
pub use payg_pricing_repository::{PAYGPricingRepository, PaygPricingRepoResult};
pub mod usage_event_repository;
pub use usage_event_repository::{UsageDailyPoint, UsageOperationPoint, UsageReport};
pub use usage_event_repository::{UsageEventRepoResult, UsageEventRepository};

pub mod payment_repository;
pub use payment_repository::{PaymentRepoResult, PaymentRepository};
EOF

mkdir -p "$(dirname "$PROJECT/crates/application/src/ports/scheduling_repository.rs")"
cat <<'EOF' > "$PROJECT/crates/application/src/ports/scheduling_repository.rs"
use std::{error::Error, ops::Deref, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Monitor, shared::ids::MonitorId};
use uuid::Uuid;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// A monitor claim together with the durable claim timestamp. The timestamp
/// is part of the billing idempotency identity: normal completion clears it,
/// while lease recovery deliberately preserves it so a crash/retry reuses the
/// same PAYG request id instead of creating a second charge.
#[derive(Debug, Clone)]
pub struct ClaimedOneTimeCheck { pub request: domain::OneTimeCheckRequest, pub monitor: Monitor }

#[derive(Debug, Clone)]
pub struct ClaimedMonitor {
    pub monitor: Monitor,
    pub claimed_at: DateTime<Utc>,
    /// Free monitoring applies only while the tenant has <=10 monitors,
    /// the wallet has no funds, and this monitor runs at the 5-minute minimum.
    pub free_entitled: bool,
    pub execution_id: Uuid,
}
impl Deref for ClaimedMonitor {
    type Target = Monitor;

    fn deref(&self) -> &Self::Target {
        &self.monitor
    }
}

/// Database-backed scheduling state for the durable worker scheduler.
///
/// The scheduler loop uses this repository to:
/// - atomically claim due monitors (no double-claim under concurrent
///   scheduler instances)
/// - release claims after execution
/// - recover leases left behind by crashed workers.
#[async_trait]
pub trait SchedulingRepository: Send + Sync {
    /// Atomically claims up to `limit` due monitors, assigning
    /// `lease_owner` and a lease that expires after `lease_duration`.
    ///
    /// The returned monitors are guaranteed to be exclusive to the
    /// caller until the lease expires or `complete_claim` is called.
    async fn claim_due_monitors(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedMonitor>>;

    /// Clears the lease fields for `monitor_id` and advances
    /// `next_due_at` by `interval_seconds` from the database's `now()`.
    /// Called after an execution attempt finishes (success or error).
    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()>;

    /// Marks all eligible monitors in a tenant due immediately without
    /// disturbing an active lease. The worker scheduler will claim them on
    /// its next pass.
    async fn request_immediate_checks(
        &self,
        tenant_id: domain::shared::ids::TenantId,
    ) -> RepoResult<u64>;

    /// Clears leases whose `lease_expires_at` is earlier than `now`,
    /// making those monitors claimable again. Returns the number of
    /// recovered leases.
    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64>;
    async fn enqueue_one_time_check(&self, tenant_id: domain::shared::ids::TenantId, monitor_id: MonitorId, created_by: domain::shared::ids::UserId, request_id: &str) -> RepoResult<domain::OneTimeCheckRequest>;
    async fn claim_one_time_checks(&self, limit:u32, lease_owner:Uuid, lease_duration:Duration, now:DateTime<Utc>) -> RepoResult<Vec<ClaimedOneTimeCheck>>;
    async fn complete_one_time_check(&self, request_id:Uuid, lease_owner:Uuid, status:&str, error:Option<&str>) -> RepoResult<()>;
    async fn get_one_time_check(&self, tenant_id:domain::shared::ids::TenantId, monitor_id:MonitorId, request_id:Uuid) -> RepoResult<Option<domain::OneTimeCheckRequest>>;
}

#[async_trait]
impl<T> SchedulingRepository for Arc<T>
where
    T: SchedulingRepository + ?Sized + Send + Sync,
{
    async fn claim_due_monitors(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedMonitor>> {
        (**self)
            .claim_due_monitors(limit, lease_owner, lease_duration, now)
            .await
    }

    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()> {
        (**self).complete_claim(monitor_id, lease_owner).await
    }

    async fn request_immediate_checks(
        &self,
        tenant_id: domain::shared::ids::TenantId,
    ) -> RepoResult<u64> {
        (**self).request_immediate_checks(tenant_id).await
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64> { (**self).recover_expired_leases(now).await }
    async fn enqueue_one_time_check(&self,t:domain::shared::ids::TenantId,m:MonitorId,u:domain::shared::ids::UserId,r:&str)->RepoResult<domain::OneTimeCheckRequest>{(**self).enqueue_one_time_check(t,m,u,r).await}
    async fn claim_one_time_checks(&self,l:u32,o:Uuid,d:Duration,n:DateTime<Utc>)->RepoResult<Vec<ClaimedOneTimeCheck>>{(**self).claim_one_time_checks(l,o,d,n).await}
    async fn complete_one_time_check(&self,r:Uuid,o:Uuid,s:&str,e:Option<&str>)->RepoResult<()>{(**self).complete_one_time_check(r,o,s,e).await}
    async fn get_one_time_check(&self,t:domain::shared::ids::TenantId,m:MonitorId,r:Uuid)->RepoResult<Option<domain::OneTimeCheckRequest>>{(**self).get_one_time_check(t,m,r).await}
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/application/src/ports/wallet_repository.rs")"
cat <<'EOF' > "$PROJECT/crates/application/src/ports/wallet_repository.rs"
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    AutoTopUpAttempt, AutoTopUpCandidate, CreditReservation, Wallet, WalletNotification,
    WalletSettings, WalletTopUpSession, WalletTransaction, WalletTransactionType,
};
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub type WalletRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationOutcome {
    pub reservation: CreditReservation,
    pub created: bool,
}

#[async_trait]
pub trait WalletRepository: Send + Sync {
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet>;
    async fn get_balance_summary(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<domain::WalletBalanceSummary>;
    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>>;

    /// Atomically checks available credits and creates (or returns) the
    /// idempotent reservation. Implementations MUST lock the wallet row and
    /// perform the uniqueness check and insert in the same transaction.
    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome>;

    /// Captures a pending reservation and writes the immutable debit ledger.
    /// When supplied, the usage event and financial audit rows are committed in
    /// the same database transaction as the debit. Replays of CAPTURED rows
    /// backfill missing event/audit rows without creating another debit.
    async fn capture(
        &self,
        reservation_id: Uuid,
        usage_event: Option<&domain::UsageEvent>,
        audit_entries: &[domain::FinancialAuditLogEntry],
    ) -> WalletRepoResult<WalletTransaction>;

    /// Releases a pending reservation without charging the wallet.
    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()>;

    /// Credits the wallet atomically. Used only by trusted payment/webhook
    /// flows; callers must provide an idempotency key.
    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction>;

    async fn create_top_up_session(&self, session: &WalletTopUpSession) -> WalletRepoResult<()>;
    async fn find_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>>;
    async fn complete_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        provider_payment_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>>;
    async fn refundable_top_up(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
    ) -> WalletRepoResult<Option<WalletTopUpSession>>;
    async fn record_verified_refund(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
        provider_refund_id: &str,
    ) -> WalletRepoResult<bool>;
    async fn create_refund_request(
        &self,
        request: &domain::WalletRefundRequest,
    ) -> WalletRepoResult<Option<domain::WalletRefundRequest>>;
    async fn set_refund_provider_id(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
        provider_refund_id: &str,
    ) -> WalletRepoResult<()>;
    async fn record_auto_top_up_attempt(&self, attempt: &AutoTopUpAttempt) -> WalletRepoResult<()>;
    async fn claim_auto_top_up(
        &self,
        tenant_id: Uuid,
        lease_id: Uuid,
        lease_for: std::time::Duration,
    ) -> WalletRepoResult<bool>;
    async fn release_auto_top_up(&self, tenant_id: Uuid, lease_id: Uuid) -> WalletRepoResult<()>;
    async fn get_settings(&self, tenant_id: Uuid) -> WalletRepoResult<WalletSettings>;
    async fn update_settings(&self, settings: &WalletSettings) -> WalletRepoResult<WalletSettings>;
    async fn expire_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<Option<WalletTransaction>>;
    async fn grant_promotional_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        expires_at: DateTime<Utc>,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction>;
    async fn expire_promotional_credits(&self, limit: u32) -> WalletRepoResult<u64>;
    async fn expire_pending_reservations(&self, limit: u32) -> WalletRepoResult<u64>;
    async fn auto_top_up_candidates(&self, limit: u32)
    -> WalletRepoResult<Vec<AutoTopUpCandidate>>;
    async fn record_balance_notification(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        dedupe_key: &str,
        message: &str,
    ) -> WalletRepoResult<bool>;
    async fn list_balance_notifications(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>>;
    async fn claim_pending_email_notifications(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>> {
        let _ = limit;
        Ok(Vec::new())
    }
    async fn mark_balance_notification_emailed(&self, id: Uuid) -> WalletRepoResult<bool> {
        let _ = id;
        Ok(false)
    }
}

#[async_trait]
impl<T> WalletRepository for Arc<T>
where
    T: WalletRepository + Send + Sync + ?Sized,
{
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        (**self).get_or_create(tenant_id, currency).await
    }
    async fn get_balance_summary(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<domain::WalletBalanceSummary> { (**self).get_balance_summary(tenant_id,currency).await }

    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>> {
        (**self).list_transactions(tenant_id, limit).await
    }
    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome> {
        (**self)
            .reserve(tenant_id, amount_units, operation, request_id, expires_at)
            .await
    }
    async fn capture(
        &self,
        reservation_id: Uuid,
        usage_event: Option<&domain::UsageEvent>,
        audit_entries: &[domain::FinancialAuditLogEntry],
    ) -> WalletRepoResult<WalletTransaction> {
        (**self)
            .capture(reservation_id, usage_event, audit_entries)
            .await
    }
    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()> {
        (**self).release(reservation_id).await
    }
    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        (**self)
            .credit(
                tenant_id,
                amount_units,
                transaction_type,
                idempotency_key,
                description,
            )
            .await
    }
    async fn create_top_up_session(&self, session: &WalletTopUpSession) -> WalletRepoResult<()> {
        (**self).create_top_up_session(session).await
    }
    async fn find_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        (**self)
            .find_top_up_session(tenant_id, provider_checkout_id)
            .await
    }
    async fn complete_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        provider_payment_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        (**self)
            .complete_top_up_session(tenant_id, provider_checkout_id, provider_payment_id)
            .await
    }
    async fn refundable_top_up(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        (**self)
            .refundable_top_up(tenant_id, provider_payment_id, amount_units)
            .await
    }
    async fn record_verified_refund(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
        provider_refund_id: &str,
    ) -> WalletRepoResult<bool> {
        (**self)
            .record_verified_refund(
                tenant_id,
                provider_payment_id,
                amount_units,
                provider_refund_id,
            )
            .await
    }
    async fn create_refund_request(
        &self,
        request: &domain::WalletRefundRequest,
    ) -> WalletRepoResult<Option<domain::WalletRefundRequest>> {
        (**self).create_refund_request(request).await
    }
    async fn set_refund_provider_id(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
        provider_refund_id: &str,
    ) -> WalletRepoResult<()> {
        (**self)
            .set_refund_provider_id(tenant_id, idempotency_key, provider_refund_id)
            .await
    }
    async fn record_auto_top_up_attempt(&self, attempt: &AutoTopUpAttempt) -> WalletRepoResult<()> {
        (**self).record_auto_top_up_attempt(attempt).await
    }
    async fn claim_auto_top_up(
        &self,
        tenant_id: Uuid,
        lease_id: Uuid,
        lease_for: std::time::Duration,
    ) -> WalletRepoResult<bool> {
        (**self)
            .claim_auto_top_up(tenant_id, lease_id, lease_for)
            .await
    }
    async fn release_auto_top_up(&self, tenant_id: Uuid, lease_id: Uuid) -> WalletRepoResult<()> {
        (**self).release_auto_top_up(tenant_id, lease_id).await
    }
    async fn get_settings(&self, tenant_id: Uuid) -> WalletRepoResult<WalletSettings> {
        (**self).get_settings(tenant_id).await
    }
    async fn update_settings(&self, settings: &WalletSettings) -> WalletRepoResult<WalletSettings> {
        (**self).update_settings(settings).await
    }
    async fn expire_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<Option<WalletTransaction>> {
        (**self)
            .expire_credit(tenant_id, amount_units, idempotency_key, description)
            .await
    }
    async fn grant_promotional_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        expires_at: DateTime<Utc>,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        (**self)
            .grant_promotional_credit(
                tenant_id,
                amount_units,
                expires_at,
                idempotency_key,
                description,
            )
            .await
    }
    async fn expire_promotional_credits(&self, limit: u32) -> WalletRepoResult<u64> {
        (**self).expire_promotional_credits(limit).await
    }
    async fn expire_pending_reservations(&self, limit: u32) -> WalletRepoResult<u64> {
        (**self).expire_pending_reservations(limit).await
    }
    async fn auto_top_up_candidates(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<AutoTopUpCandidate>> {
        (**self).auto_top_up_candidates(limit).await
    }
    async fn record_balance_notification(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        dedupe_key: &str,
        message: &str,
    ) -> WalletRepoResult<bool> {
        (**self)
            .record_balance_notification(tenant_id, event_type, dedupe_key, message)
            .await
    }
    async fn list_balance_notifications(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>> {
        (**self).list_balance_notifications(tenant_id, limit).await
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/application/src/use_cases/create_monitor.rs")"
cat <<'EOF' > "$PROJECT/crates/application/src/use_cases/create_monitor.rs"
use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    dto::CreateMonitorRequest,
    ports::{
        AuditLogEntryRepository, IdGenerator, MonitorRepository, QuotaCheckOutcome, WalletRepository,
    },
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, ExpectedStatus, HeartbeatToken,
    Host, Monitor, MonitorBuilder, MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    shared::ids::{TenantId, UserId},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Free-tier monitor cap. Funded PAYG wallets have no product-defined monitor count ceiling.
pub const MAX_FREE_MONITORS: usize = 10;
pub const MAX_MONITORS_PER_USER: usize = MAX_FREE_MONITORS;

/// Backwards-compatible fallback minimum interval for direct/unit-test
/// construction without an entitlement repository. This mirrors the current
/// Free-tier contract; funded PAYG wallets use the domain minimum interval.
const DEFAULT_FREE_MIN_INTERVAL_SECONDS: u32 = 300;

/// Hard cap on the number of assertions a single monitor may be created with.
pub const MAX_ASSERTIONS_PER_MONITOR: usize = 20;

#[derive(Debug)]
pub enum CreateMonitorError {
    Forbidden,
    QuotaExceeded { limit: usize },
    TooManyAssertions { limit: usize },
    IntervalBelowPlanMinimum { min: u32 },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateMonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to create monitors"
            ),
            Self::QuotaExceeded { limit } => {
                write!(
                    f,
                    "Free entitlement allows at most {limit} monitors; fund the PAYG wallet to add more."
                )
            }
            Self::TooManyAssertions { limit } => {
                write!(
                    f,
                    "too many assertions: a monitor allows at most {limit} assertions"
                )
            }
            Self::IntervalBelowPlanMinimum { min } => {
                write!(
                    f,
                    "interval must be at least {min} seconds for the current billing eligibility state"
                )
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateMonitorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Forbidden => None,
            Self::QuotaExceeded { .. } => None,
            Self::TooManyAssertions { .. } => None,
            Self::IntervalBelowPlanMinimum { .. } => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct CreateMonitorUseCase<R, G> {
    repository: R,
    id_generator: G,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    wallet_repository: Option<Arc<dyn WalletRepository>>,
}

impl<R, G> CreateMonitorUseCase<R, G>
where
    R: MonitorRepository,
    G: IdGenerator,
{
    pub fn new(
        repository: R,
        id_generator: G,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            id_generator,
            audit_log_entry_repository,
            wallet_repository: None,
        }
    }

    pub fn with_wallet_repository(mut self, repository: Arc<dyn WalletRepository>) -> Self { self.wallet_repository=Some(repository); self }

    pub async fn execute(
        &self,
        request: CreateMonitorRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Monitor, CreateMonitorError> {
        if !authenticated_user.can(Permission::CreateMonitor) {
            return Err(CreateMonitorError::Forbidden);
        }

        if request.assertions.len() > MAX_ASSERTIONS_PER_MONITOR {
            return Err(CreateMonitorError::TooManyAssertions {
                limit: MAX_ASSERTIONS_PER_MONITOR,
            });
        }

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let funded = if let Some(wallets) = &self.wallet_repository {
            wallets.get_or_create(tenant_id.0, "USD").await.map_err(CreateMonitorError::Other)?;
            wallets.get_balance_summary(tenant_id.0, "USD").await.map_err(CreateMonitorError::Other)?.available_units > 0
        } else { false };
        let min_interval = if funded { CheckInterval::MIN } else { DEFAULT_FREE_MIN_INTERVAL_SECONDS };
        if request.interval_seconds < min_interval {
            return Err(CreateMonitorError::IntervalBelowPlanMinimum { min: min_interval });
        }

        let monitor = self
            .build_monitor(request, &authenticated_user)
            .await
            .map_err(CreateMonitorError::Other)?;

        if funded {
            self.repository.save(&monitor).await.map_err(CreateMonitorError::Other)?;
        } else {
            match self.repository.save_if_within_quota(&monitor, tenant_id, MAX_FREE_MONITORS as u32).await.map_err(CreateMonitorError::Other)? {
                QuotaCheckOutcome::Saved => {}
                QuotaCheckOutcome::QuotaExceeded => return Err(CreateMonitorError::QuotaExceeded { limit: MAX_FREE_MONITORS }),
            }
        }

        self.record_audit_log(&monitor, user_id, tenant_id)
            .await
            .map_err(CreateMonitorError::Other)?;

        Ok(monitor)
    }

    async fn build_monitor(
        &self,
        request: CreateMonitorRequest,
        authenticated_user: &AuthenticatedUser,
    ) -> UseCaseResult<Monitor> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let id = self.id_generator.next_monitor_id().await;

        let target = match request.kind {
            MonitorKind::Http | MonitorKind::Heartbeat => {
                Url::new(request.url)?.as_str().to_owned()
            }
            MonitorKind::Tcp | MonitorKind::Dns => Host::new(request.url)?.as_str().to_owned(),
            MonitorKind::Flow => request.url,
        };

        let mut builder = MonitorBuilder::new(id, user_id, tenant_id)
            .name(MonitorName::new(request.name)?)
            .target(target)
            .method(request.method)
            .expected_status(ExpectedStatus::new(request.expected_status)?)
            .interval(CheckInterval::new(request.interval_seconds)?)
            .timeout(Timeout::new(request.timeout_seconds)?)
            .kind(request.kind);

        if let Some(tcp_port) = request.tcp_port {
            builder = builder.tcp_port(tcp_port);
        }

        if let Some(dns_record_type) = request.dns_record_type {
            builder = builder.dns_record_type(dns_record_type);
        }

        if let Some(dns_expected_value) = request.dns_expected_value {
            builder = builder.dns_expected_value(dns_expected_value);
        }

        if request.kind == MonitorKind::Heartbeat {
            builder = builder.heartbeat_token(HeartbeatToken::generate());

            if let Some(interval) = request.heartbeat_interval_seconds {
                builder = builder.heartbeat_interval_seconds(interval);
            }

            if let Some(grace) = request.heartbeat_grace_period_seconds {
                builder = builder.heartbeat_grace_period_seconds(grace);
            }
        }

        let mut monitor = builder.try_build()?;

        monitor.is_public = request.is_public;
        monitor.ssl_check_enabled = request.ssl_check_enabled;
        monitor.ssl_expiry_warning_days = request.ssl_expiry_warning_days;

        for input in &request.assertions {
            let assertion = Assertion::new(
                input.response_part.clone(),
                input.operator.clone(),
                input.target.clone(),
                input.expected_value.clone(),
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

            monitor
                .add_assertion(assertion)
                .map_err(Box::<dyn Error + Send + Sync>::from)?;
        }

        Ok(monitor)
    }

    async fn record_audit_log(
        &self,
        monitor: &Monitor,
        user_id: UserId,
        tenant_id: TenantId,
    ) -> UseCaseResult<()> {
        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MonitorCreated,
            target_id: monitor.id.0.to_string(),
            metadata: json!({
                "name": monitor.name.as_str(),
                "is_public": monitor.is_public,
            }),
            created_at: Utc::now(),
        };

        self.audit_log_entry_repository.create(&entry).await?;
        Ok(())
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/application/src/use_cases/update_monitor.rs")"
cat <<'EOF' > "$PROJECT/crates/application/src/use_cases/update_monitor.rs"
use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::UpdateMonitor,
    ports::{
        AuditLogEntryRepository, MonitorRepository, WalletRepository,
    },
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, ExpectedStatus, Host, Monitor,
    MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct UpdateMonitorUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    wallet_repository: Option<Arc<dyn WalletRepository>>,
}

impl<R> UpdateMonitorUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(
        repository: R,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            audit_log_entry_repository,
            wallet_repository: None,
        }
    }

    pub fn with_wallet_repository(mut self, repository: Arc<dyn WalletRepository>) -> Self { self.wallet_repository=Some(repository); self }

    pub async fn execute(
        &self,
        command: UpdateMonitor,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<Monitor>> {
        authenticated_user.require(Permission::UpdateMonitor)?;

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut monitor = match self.repository.find_by_id(command.id).await? {
            Some(monitor) => monitor,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Ok(None);
        }

        let funded = if let Some(wallets) = &self.wallet_repository {
            wallets.get_or_create(tenant_id.0, "USD").await?;
            wallets.get_balance_summary(tenant_id.0, "USD").await?.available_units > 0
        } else { false };
        let min_interval = if funded { CheckInterval::MIN } else { 300 };

        if command.interval_seconds < min_interval {
            return Err(format!(
                "interval must be at least {min_interval} seconds for the current billing eligibility state"
            )
            .into());
        }

        // Validate target based on kind, same as create
        let target = match command.kind {
            MonitorKind::Http | MonitorKind::Heartbeat => {
                Url::new(command.url)?.as_str().to_owned()
            }
            MonitorKind::Tcp | MonitorKind::Dns => Host::new(command.url)?.as_str().to_owned(),
            MonitorKind::Flow => command.url,
        };

        monitor.update(
            MonitorName::new(command.name)?,
            target,
            command.method,
            ExpectedStatus::new(command.expected_status)?,
            CheckInterval::new(command.interval_seconds)?,
            Timeout::new(command.timeout_seconds)?,
        );

        monitor
            .update_kind_fields(
                command.kind,
                command.tcp_port,
                command.dns_record_type,
                command.dns_expected_value,
                command.heartbeat_interval_seconds,
                command.heartbeat_grace_period_seconds,
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

        monitor.is_public = command.is_public;
        monitor.ssl_check_enabled = command.ssl_check_enabled;
        monitor.ssl_expiry_warning_days = command.ssl_expiry_warning_days;

        monitor.assertions.clear();

        for input in &command.assertions {
            let assertion = Assertion::new(
                input.response_part.clone(),
                input.operator.clone(),
                input.target.clone(),
                input.expected_value.clone(),
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

            monitor
                .add_assertion(assertion)
                .map_err(Box::<dyn Error + Send + Sync>::from)?;
        }

        self.repository.save(&monitor).await?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MonitorUpdated,
            target_id: monitor.id.0.to_string(),
            metadata: json!({
                "name": monitor.name.as_str(),
                "is_public": monitor.is_public,
            }),
            created_at: Utc::now(),
        };

        self.audit_log_entry_repository.create(&entry).await?;

        Ok(Some(monitor))
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/application/tests/create_monitor_use_case.rs")"
cat <<'EOF' > "$PROJECT/crates/application/tests/create_monitor_use_case.rs"
mod common;
use common::*;

use application::dto::CreateMonitorRequest;
use application::use_cases::{CreateMonitorError, CreateMonitorUseCase};
use domain::HttpMethod;
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, UuidIdGenerator};

use uuid::Uuid;

#[tokio::test]
async fn create_monitor_persists_monitor() {
    let repository = InMemoryMonitorRepository::new();
    let use_case =
        CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new(), new_audit_repo());
    let user_id = UserId(Uuid::new_v4());

    let request = CreateMonitorRequest {
        name: "Google".into(),
        url: "https://google.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 300,
        timeout_seconds: 10,
        assertions: vec![],
        is_public: false,
        ssl_check_enabled: true,
        ssl_expiry_warning_days: 14,
        kind: domain::MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
    };

    let monitor = use_case
        .execute(request, auth_user(user_id, tenant_id()))
        .await
        .unwrap();
    assert_eq!(monitor.name.as_str(), "Google");
}

#[tokio::test]
async fn free_tier_enforces_300_second_minimum_and_10_monitor_quota() {
    let repository = InMemoryMonitorRepository::new();
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo());
    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    let request = |name: String, interval_seconds: u32| CreateMonitorRequest {
        name,
        url: "https://example.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds,
        timeout_seconds: 10,
        assertions: vec![],
        is_public: false,
        ssl_check_enabled: true,
        ssl_expiry_warning_days: 14,
        kind: domain::MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
    };

    let below_minimum = use_case
        .execute(
            request("free-299".into(), 299),
            auth_user(user_id, tenant_id),
        )
        .await;
    assert!(matches!(
        below_minimum,
        Err(CreateMonitorError::IntervalBelowPlanMinimum { min: 300 })
    ));

    for index in 0..10 {
        use_case
            .execute(
                request(format!("free-{index}"), 300),
                auth_user(user_id, tenant_id),
            )
            .await
            .unwrap();
    }

    let over_quota = use_case
        .execute(
            request("free-11".into(), 300),
            auth_user(user_id, tenant_id),
        )
        .await;
    assert!(matches!(
        over_quota,
        Err(CreateMonitorError::QuotaExceeded { limit: 10 })
    ));
}

#[tokio::test]
async fn funded_payg_wallet_has_no_product_monitor_ceiling_and_allows_fast_interval() {
 use application::ports::WalletRepository; use domain::WalletTransactionType; use infra::InMemoryWalletRepository;
 let repository=InMemoryMonitorRepository::new(); let wallet=std::sync::Arc::new(InMemoryWalletRepository::default()); let tid=tenant_id(); wallet.get_or_create(tid.0,"USD").await.unwrap(); wallet.credit(tid.0,5_000_000,WalletTransactionType::TopUp,"test-payg-fund",Some("test")).await.unwrap();
 let use_case=CreateMonitorUseCase::new(repository,UuidIdGenerator::new(),new_audit_repo()).with_wallet_repository(wallet); let uid=UserId(Uuid::new_v4()); let request=CreateMonitorRequest{name:"fast".into(),url:"https://example.com".into(),method:HttpMethod::Get,expected_status:200,interval_seconds:30,timeout_seconds:10,assertions:vec![],is_public:false,ssl_check_enabled:true,ssl_expiry_warning_days:14,kind:domain::MonitorKind::Http,tcp_port:None,dns_record_type:None,dns_expected_value:None,heartbeat_interval_seconds:None,heartbeat_grace_period_seconds:None}; use_case.execute(request,auth_user(uid,tid)).await.unwrap();
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/domain/src/entities/mod.rs")"
cat <<'EOF' > "$PROJECT/crates/domain/src/entities/mod.rs"
mod financial_audit_log;
pub use financial_audit_log::FinancialAuditLogEntry;
pub mod assertion;
pub use assertion::*;
pub mod api_flow_step;
pub use api_flow_step::ApiFlowStep;
pub mod api_flow;
pub use api_flow::ApiFlow;
pub mod proxy_entry;
pub use proxy_entry::ProxyEntry;
pub mod proxy_check_job;
pub use proxy_check_job::ProxyCheckJob;
pub mod diagnostic_report;
pub use diagnostic_report::*;

// Sabbin modules:
pub mod escalation_policy;
pub use escalation_policy::{EscalationPolicy, EscalationStep};

mod maintenance_window;
pub use maintenance_window::MaintenanceWindow;

mod status_page;
pub use status_page::*;

mod tenant;
pub use tenant::Tenant;
pub mod one_time_check;
pub use one_time_check::OneTimeCheckRequest;

mod tenant_membership;
pub use tenant_membership::TenantMembership;

mod audit_log_entry;
pub use audit_log_entry::AuditLogEntry;

mod api_key;
pub use api_key::{ApiKey, CreateApiKey};

mod backup_code;
pub use backup_code::BackupCode;

// B5 — Email Verification:
mod email_verification_token;
pub use email_verification_token::EmailVerificationToken;

// B6 — Password reset:
mod reset_password_token;
pub use reset_password_token::ResetPasswordToken;

// B9 — Tenant invitations:
mod tenant_invitation;
pub use tenant_invitation::TenantInvitation;

// B11 — Plan / Subscription / Entitlement model:
mod plan;
pub use plan::Plan;

mod subscription;
pub use subscription::Subscription;
mod subscription_plan_change;
pub use subscription_plan_change::*;

// B14 — Billing Provider Abstraction & Checkout:
mod checkout_session;
pub use checkout_session::CheckoutSession;

// B15 — Billing Webhooks:
pub mod billing_audit_log_entry;
pub mod billing_webhook_event;
pub use billing_audit_log_entry::BillingAuditLogEntry;
pub use billing_webhook_event::BillingWebhookEvent;

// H9 — Pricing Audit Log:
mod pricing_audit_log_entry;
pub use pricing_audit_log_entry::PricingAuditLogEntry;

mod notification_outbox_entry;
pub use notification_outbox_entry::NotificationOutboxEntry;

// H12 — Proration Service:
mod subscription_usage_period;
pub use subscription_usage_period::SubscriptionUsagePeriod;

// Gyara: an manta da bayyana wadannan modules
mod check;
pub use check::Check;
mod check_result;
pub use check_result::CheckResult;
mod monitor;
pub use monitor::*;
mod monitor_builder;
pub use monitor_builder::MonitorBuilder;
mod user;
pub use user::User;
mod user_builder;
pub use user_builder::UserBuilder;

// Gyara: an manta da bayyana wadannan modules
mod incident;
pub use incident::*;
mod notification_channel;
pub use notification_channel::{CreateEmailChannel, CreateWebhookChannel, NotificationChannel};
mod subscriber;
pub use subscriber::*;

// H14 — Help Center & Support:
mod help_article;
mod help_category;
mod support_message;
mod support_ticket;

pub use help_article::{CreateHelpArticle, HelpArticle};
pub use help_category::{CreateHelpCategory, HelpCategory};
pub use support_message::{CreateSupportMessage, SupportMessage};
pub use support_ticket::{CreateSupportTicket, SupportTicket};

mod add_on_subscription;
pub use add_on_subscription::{AddOnStatus, AddOnSubscription, AddOnType};

// RUM (Real User Monitoring):
mod rum_site;
pub use rum_site::RumSite;
mod rum_session;
pub use rum_session::{DeviceType, RumSession};
mod rum_page_view;
pub use rum_page_view::{RumPageView, RumVitals};
mod rum_js_error;
pub use rum_js_error::RumJsError;
mod rum_resource_error;
pub use rum_resource_error::{RumResourceError, RumResourceType};

pub mod wallet;
pub use wallet::{
    AutoTopUpAttempt, AutoTopUpCandidate, CreditReservation, Wallet, WalletBalanceSummary, WalletNotification,
    WalletRefundRequest, WalletSettings, WalletTopUpSession, WalletTransaction,
    WalletTransactionType,
};

mod payg;
pub use payg::{PAYGPricing, Payment, UsageEvent};
mod invoice;
pub use invoice::{Invoice, InvoiceLine, InvoiceLineType, InvoiceSourceType, InvoiceStatus};
EOF

mkdir -p "$(dirname "$PROJECT/crates/domain/src/entities/one_time_check.rs")"
cat <<'EOF' > "$PROJECT/crates/domain/src/entities/one_time_check.rs"
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneTimeCheckRequest {
    pub id: Uuid, pub tenant_id: Uuid, pub monitor_id: Uuid, pub created_by: Uuid,
    pub request_id: String, pub status: String, pub execution_id: Uuid,
    pub lease_owner: Option<Uuid>, pub lease_expires_at: Option<DateTime<Utc>>,
    pub error: Option<String>, pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>, pub completed_at: Option<DateTime<Utc>>,
}
impl OneTimeCheckRequest {
    pub const QUEUED: &'static str = "QUEUED";
    pub const RUNNING: &'static str = "RUNNING";
    pub const SUCCEEDED: &'static str = "SUCCEEDED";
    pub const FAILED: &'static str = "FAILED";
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/domain/src/entities/wallet.rs")"
cat <<'EOF' > "$PROJECT/crates/domain/src/entities/wallet.rs"
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalletTransactionType {
    TopUp,
    Debit,
    Refund,
    Adjustment,
    PromotionalCredit,
    Expiration,
}
impl WalletTransactionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopUp => "TOP_UP",
            Self::Debit => "DEBIT",
            Self::Refund => "REFUND",
            Self::Adjustment => "ADJUSTMENT",
            Self::PromotionalCredit => "PROMOTIONAL_CREDIT",
            Self::Expiration => "EXPIRATION",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wallet {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub currency: String,
    pub balance_units: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletBalanceSummary {
    pub balance_units: i64,
    pub reserved_units: i64,
    pub available_units: i64,
}

impl Wallet {
    pub fn new(id: Uuid, tenant_id: Uuid, currency: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id,
            tenant_id,
            currency: currency.into(),
            balance_units: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletTransaction {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub wallet_id: Uuid,
    pub transaction_type: WalletTransactionType,
    pub amount_units: i64,
    pub balance_after_units: i64,
    pub idempotency_key: String,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditReservation {
    pub id: Uuid,
    pub wallet_id: Uuid,
    pub amount_units: i64,
    pub status: String,
    pub operation: String,
    pub request_id: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl CreditReservation {
    pub const PENDING: &'static str = "PENDING";
    pub const CAPTURED: &'static str = "CAPTURED";
    pub const RELEASED: &'static str = "RELEASED";
    pub const EXPIRED: &'static str = "EXPIRED";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletSettings {
    pub tenant_id: Uuid,
    pub low_balance_threshold_units: i64,
    pub critical_balance_threshold_units: i64,
    pub auto_top_up_enabled: bool,
    pub auto_top_up_threshold_units: i64,
    pub auto_top_up_amount_units: i64,
    pub payment_method: Option<String>,
    pub daily_top_up_limit_units: i64,
    pub monthly_top_up_limit_units: i64,
    pub retry_limit: i32,
}

impl Default for WalletSettings {
    fn default() -> Self {
        Self {
            tenant_id: Uuid::nil(),
            low_balance_threshold_units: 5_000_000,
            critical_balance_threshold_units: 1_000_000,
            auto_top_up_enabled: false,
            auto_top_up_threshold_units: 5_000_000,
            auto_top_up_amount_units: 20_000_000,
            payment_method: None,
            daily_top_up_limit_units: 100_000_000,
            monthly_top_up_limit_units: 500_000_000,
            retry_limit: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoTopUpCandidate {
    pub tenant_id: Uuid,
    pub balance_units: i64,
    pub settings: WalletSettings,
    pub daily_spend_units: i64,
    pub monthly_spend_units: i64,
    pub daily_attempts: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletNotification {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub event_type: String,
    pub dedupe_key: String,
    pub message: String,
    pub read_at: Option<DateTime<Utc>>,
    pub email_sent_at: Option<DateTime<Utc>>,
    pub email_processing_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletTopUpSession {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider: String,
    pub provider_checkout_id: String,
    pub provider_payment_id: Option<String>,
    pub amount_units: i64,
    pub refunded_amount_units: i64,
    pub currency: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WalletTopUpSession {
    pub const PENDING: &'static str = "PENDING";
    pub const COMPLETED: &'static str = "COMPLETED";
    pub const FAILED: &'static str = "FAILED";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoTopUpAttempt {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub amount_units: i64,
    pub status: String,
    pub provider_reference: Option<String>,
    pub error_message: Option<String>,
    pub attempted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletRefundRequest {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider_payment_id: String,
    pub amount_units: i64,
    pub idempotency_key: String,
    pub provider_refund_id: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/domain/src/lib.rs")"
cat <<'EOF' > "$PROJECT/crates/domain/src/lib.rs"
pub mod entities;
pub mod enums;
pub mod errors;
pub mod events;
pub mod multi_region;
pub mod notification;
pub mod repositories;
pub mod shared;
pub mod value_objects;

pub use entities::*;
pub use enums::*;
pub use errors::*;
pub use events::*;
pub use multi_region::*;
pub use notification::*;
pub use repositories::*;
pub use shared::ids::*;
pub use value_objects::*;

pub mod pricing;
pub use pricing::*;

pub mod proration;
pub use proration::*;

pub mod payg;
pub use payg::*;

// Worker 6 billing invoice exports
pub use entities::{Invoice, InvoiceLine, InvoiceLineType, InvoiceSourceType, InvoiceStatus};
EOF

mkdir -p "$(dirname "$PROJECT/crates/domain/src/payg.rs")"
cat <<'EOF' > "$PROJECT/crates/domain/src/payg.rs"
use crate::MonitorKind;

/// Canonical server-owned PAYG operation identifiers. Runtime code derives
/// these values; clients never submit prices or operation identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaygOperation {
    HttpOneTime, TcpOneTime, DnsOneTime, ApiFlowOneTime, ProxyOneTime,
    HttpMonitoring, TcpMonitoring, DnsMonitoring, HeartbeatProcessing,
    ApiFlowMonitoring, ProxyMonitoring, RumEvent, Sms, Storage, NetworkEgress,
}
impl PaygOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HttpOneTime => "http_check.one_time",
            Self::TcpOneTime => "tcp_check.one_time",
            Self::DnsOneTime => "dns_check.one_time",
            Self::ApiFlowOneTime => "api_flow_check.one_time",
            Self::ProxyOneTime => "proxy_check.one_time",
            Self::HttpMonitoring => "http_check.monitoring",
            Self::TcpMonitoring => "tcp_check.monitoring",
            Self::DnsMonitoring => "dns_check.monitoring",
            Self::HeartbeatProcessing => "heartbeat_processing",
            Self::ApiFlowMonitoring => "api_flow_check.monitoring",
            Self::ProxyMonitoring => "proxy_check.monitoring",
            Self::RumEvent => "rum_event",
            Self::Sms => "sms",
            Self::Storage => "storage",
            Self::NetworkEgress => "network_egress",
        }
    }
}
pub const fn monitor_operation(kind: MonitorKind, one_time: bool) -> PaygOperation {
    match (kind, one_time) {
        (MonitorKind::Http, true) => PaygOperation::HttpOneTime,
        (MonitorKind::Tcp, true) => PaygOperation::TcpOneTime,
        (MonitorKind::Dns, true) => PaygOperation::DnsOneTime,
        (MonitorKind::Flow, true) => PaygOperation::ApiFlowOneTime,
        (MonitorKind::Http, false) => PaygOperation::HttpMonitoring,
        (MonitorKind::Tcp, false) => PaygOperation::TcpMonitoring,
        (MonitorKind::Dns, false) => PaygOperation::DnsMonitoring,
        (MonitorKind::Flow, false) => PaygOperation::ApiFlowMonitoring,
        (MonitorKind::Heartbeat, _) => PaygOperation::HeartbeatProcessing,
    }
}

#[cfg(test)]
mod tests { use super::*; #[test] fn operations_are_distinct(){assert_eq!(monitor_operation(MonitorKind::Http,false).as_str(),"http_check.monitoring");assert_eq!(monitor_operation(MonitorKind::Http,true).as_str(),"http_check.one_time");assert_ne!(monitor_operation(MonitorKind::Http,false).as_str(),monitor_operation(MonitorKind::Http,true).as_str());} }
EOF

mkdir -p "$(dirname "$PROJECT/crates/infra/src/repositories/in_memory_wallet_repository.rs")"
cat <<'EOF' > "$PROJECT/crates/infra/src/repositories/in_memory_wallet_repository.rs"
use application::ports::{ReservationOutcome, WalletRepoResult, WalletRepository};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use domain::{
    AutoTopUpAttempt, CreditReservation, Wallet, WalletNotification, WalletSettings,
    WalletTopUpSession, WalletTransaction, WalletTransactionType,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
struct Promo {
    id: Uuid,
    tenant_id: Uuid,
    remaining: i64,
    expires_at: DateTime<Utc>,
}

type WalletTransactions = Vec<WalletTransaction>;
type WalletReservations = HashMap<Uuid, CreditReservation>;
type WalletStore = HashMap<Uuid, (Wallet, WalletTransactions, WalletReservations)>;
type WalletClaimStore = HashMap<Uuid, (Uuid, DateTime<Utc>)>;

#[derive(Clone, Default)]
pub struct InMemoryWalletRepository {
    inner: Arc<Mutex<WalletStore>>,
    settings: Arc<Mutex<HashMap<Uuid, WalletSettings>>>,
    promos: Arc<Mutex<Vec<Promo>>>,
    sessions: Arc<Mutex<HashMap<String, WalletTopUpSession>>>,
    attempts: Arc<Mutex<Vec<AutoTopUpAttempt>>>,
    notifications: Arc<Mutex<Vec<WalletNotification>>>,

    claims: Arc<Mutex<WalletClaimStore>>,
    refunds: Arc<Mutex<HashMap<String, domain::WalletRefundRequest>>>,
}

#[async_trait]
impl WalletRepository for InMemoryWalletRepository {
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        let mut g = self.inner.lock().unwrap();
        let (wallet, _, _) = g.entry(tenant_id).or_insert_with(|| {
            (
                Wallet::new(Uuid::new_v4(), tenant_id, currency),
                Vec::new(),
                HashMap::new(),
            )
        });
        Ok(wallet.clone())
    }

    async fn get_balance_summary(&self, tenant_id: Uuid, _currency: &str) -> WalletRepoResult<domain::WalletBalanceSummary> {
        let g=self.inner.lock().unwrap();
        let (wallet,_,reservations)=g.get(&tenant_id).ok_or("wallet not found")?;
        let reserved=reservations.values().filter(|r|r.status==CreditReservation::PENDING && r.expires_at>Utc::now()).map(|r|r.amount_units).sum::<i64>();
        Ok(domain::WalletBalanceSummary{balance_units:wallet.balance_units,reserved_units:reserved,available_units:wallet.balance_units.saturating_sub(reserved)})
    }

    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>> {
        let g = self.inner.lock().unwrap();
        Ok(g.get(&tenant_id)
            .map(|(_, txs, _)| {
                txs.iter()
                    .rev()
                    .take(limit.min(100) as usize)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome> {
        if amount_units <= 0 || request_id.trim().is_empty() {
            return Err("invalid reservation".into());
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, _, reservations) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        if let Some(existing) = reservations
            .values_mut()
            .find(|r| r.request_id == request_id)
        {
            if (existing.status == CreditReservation::PENDING && existing.expires_at <= Utc::now())
                || existing.status == CreditReservation::EXPIRED
            {
                existing.amount_units = amount_units;
                existing.operation = operation.to_owned();
                existing.expires_at = expires_at;
                existing.status = CreditReservation::PENDING.to_owned();
                existing.updated_at = Utc::now();
                return Ok(ReservationOutcome {
                    reservation: existing.clone(),
                    created: true,
                });
            }
            return Ok(ReservationOutcome {
                reservation: existing.clone(),
                created: false,
            });
        }
        let reserved = reservations
            .values()
            .filter(|r| r.status == CreditReservation::PENDING && r.expires_at > Utc::now())
            .map(|r| r.amount_units)
            .sum::<i64>();
        if wallet.balance_units - reserved < amount_units {
            return Err("insufficient wallet balance".into());
        }
        let r = CreditReservation {
            id: Uuid::new_v4(),
            wallet_id: wallet.id,
            amount_units,
            status: CreditReservation::PENDING.to_owned(),
            operation: operation.to_owned(),
            request_id: request_id.to_owned(),
            expires_at,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        reservations.insert(r.id, r.clone());
        Ok(ReservationOutcome {
            reservation: r,
            created: true,
        })
    }

    async fn capture(
        &self,
        reservation_id: Uuid,
        _usage_event: Option<&domain::UsageEvent>,
        _audit_entries: &[domain::FinancialAuditLogEntry],
    ) -> WalletRepoResult<WalletTransaction> {
        let mut g = self.inner.lock().unwrap();
        for (wallet, txs, reservations) in g.values_mut() {
            if let Some(r) = reservations.get_mut(&reservation_id) {
                if r.status == CreditReservation::CAPTURED {
                    return txs
                        .iter()
                        .find(|t| t.idempotency_key == format!("reservation:{reservation_id}"))
                        .cloned()
                        .ok_or_else(|| "captured ledger entry missing".into());
                }
                if r.status != CreditReservation::PENDING || r.expires_at <= Utc::now() {
                    return Err("reservation is not capturable".into());
                }
                if wallet.balance_units < r.amount_units {
                    return Err("insufficient wallet balance".into());
                }

                let amount = r.amount_units;
                let tenant_id = wallet.tenant_id;
                let operation = r.operation.clone();
                wallet.balance_units -= amount;
                wallet.updated_at = Utc::now();
                r.status = CreditReservation::CAPTURED.to_owned();
                r.updated_at = Utc::now();

                drop(g);
                // Promo consumption is synchronized separately. The wallet balance
                // was already atomically reduced under the repository mutex.
                let mut remaining = amount;
                let mut promos = self.promos.lock().unwrap();
                let now = Utc::now();
                promos.retain(|p| p.remaining > 0 && p.expires_at > now);
                for p in promos.iter_mut().filter(|p| p.tenant_id == tenant_id) {
                    if remaining == 0 {
                        break;
                    }
                    let used = p.remaining.min(remaining);
                    p.remaining -= used;
                    remaining -= used;
                }
                drop(promos);
                let mut g = self.inner.lock().unwrap();
                let (wallet, txs, _) = g.get_mut(&tenant_id).unwrap();
                let tx = WalletTransaction {
                    id: Uuid::new_v4(),
                    tenant_id: wallet.tenant_id,
                    wallet_id: wallet.id,
                    transaction_type: WalletTransactionType::Debit,
                    amount_units: -amount,
                    balance_after_units: wallet.balance_units,
                    idempotency_key: format!("reservation:{reservation_id}"),
                    reference_type: Some("credit_reservation".to_owned()),
                    reference_id: Some(reservation_id.to_string()),
                    description: Some(operation),
                    created_at: Utc::now(),
                };
                txs.push(tx.clone());
                return Ok(tx);
            }
        }
        Err("reservation not found".into())
    }

    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()> {
        let mut g = self.inner.lock().unwrap();
        for (_, _, reservations) in g.values_mut() {
            if let Some(r) = reservations.get_mut(&reservation_id) {
                if r.status == CreditReservation::PENDING {
                    r.status = CreditReservation::RELEASED.to_owned();
                    r.updated_at = Utc::now();
                }
                return Ok(());
            }
        }
        Err("reservation not found".into())
    }

    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if amount_units <= 0 || idempotency_key.trim().is_empty() {
            return Err("invalid credit".into());
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, txs, _) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        if let Some(existing) = txs.iter().find(|t| t.idempotency_key == idempotency_key) {
            return Ok(existing.clone());
        }
        wallet.balance_units = wallet
            .balance_units
            .checked_add(amount_units)
            .ok_or("wallet balance overflow")?;
        wallet.updated_at = Utc::now();
        let tx = WalletTransaction {
            id: Uuid::new_v4(),
            tenant_id,
            wallet_id: wallet.id,
            transaction_type,
            amount_units,
            balance_after_units: wallet.balance_units,
            idempotency_key: idempotency_key.to_owned(),
            reference_type: None,
            reference_id: None,
            description: description.map(str::to_owned),
            created_at: Utc::now(),
        };
        txs.push(tx.clone());
        Ok(tx)
    }

    async fn create_top_up_session(&self, session: &WalletTopUpSession) -> WalletRepoResult<()> {
        let mut g = self.sessions.lock().unwrap();
        if g.values().any(|x| {
            x.provider == session.provider
                && x.provider_checkout_id == session.provider_checkout_id
                && x.tenant_id != session.tenant_id
        }) {
            return Err("provider checkout already belongs to another tenant".into());
        }
        g.insert(session.provider_checkout_id.clone(), session.clone());
        Ok(())
    }

    async fn find_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .get(provider_checkout_id)
            .filter(|x| x.tenant_id == tenant_id)
            .cloned())
    }

    async fn complete_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        provider_payment_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        let mut g = self.sessions.lock().unwrap();
        let Some(s) = g.get_mut(provider_checkout_id) else {
            return Ok(None);
        };
        if s.tenant_id != tenant_id {
            return Ok(None);
        }
        if s.status == WalletTopUpSession::COMPLETED {
            return Ok(Some(s.clone()));
        }
        s.status = WalletTopUpSession::COMPLETED.to_owned();
        s.provider_payment_id = Some(provider_payment_id.to_owned());
        s.updated_at = Utc::now();
        Ok(Some(s.clone()))
    }

    async fn refundable_top_up(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .values()
            .find(|s| {
                s.tenant_id == tenant_id
                    && s.provider_payment_id.as_deref() == Some(provider_payment_id)
                    && s.status == WalletTopUpSession::COMPLETED
                    && s.amount_units.saturating_sub(s.refunded_amount_units) >= amount_units
            })
            .cloned())
    }

    async fn record_verified_refund(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
        provider_refund_id: &str,
    ) -> WalletRepoResult<bool> {
        {
            let g = self.inner.lock().unwrap();
            if g.get(&tenant_id)
                .map(|(_, txs, _)| {
                    txs.iter().any(|t| {
                        t.reference_type.as_deref() == Some("provider_refund")
                            && t.reference_id.as_deref() == Some(provider_refund_id)
                    })
                })
                .unwrap_or(false)
            {
                return Ok(true);
            }
        }

        // Keep the sessions mutex strictly inside this scope.
        // Nothing borrowed from the guard is carried into an await.
        let valid = {
            let mut sessions = self.sessions.lock().unwrap();

            let Some(session) = sessions.values_mut().find(|s| {
                s.tenant_id == tenant_id
                    && s.provider_payment_id.as_deref() == Some(provider_payment_id)
                    && s.status == WalletTopUpSession::COMPLETED
            }) else {
                return Ok(false);
            };

            if amount_units <= 0
                || session.refunded_amount_units.saturating_add(amount_units) > session.amount_units
            {
                return Ok(false);
            }

            session.refunded_amount_units += amount_units;
            session.updated_at = Utc::now();

            true
        };

        if !valid {
            return Ok(false);
        }

        let _ = self
            .credit(
                tenant_id,
                amount_units,
                WalletTransactionType::Refund,
                &format!("refund:{provider_refund_id}"),
                Some("Verified payment refund"),
            )
            .await?;

        // Update the refund request only after the wallet credit has
        // completed. This mutex is also fully scoped before returning.
        {
            let mut refunds = self.refunds.lock().unwrap();

            if let Some(r) = refunds.values_mut().find(|r| {
                r.tenant_id == tenant_id
                    && r.provider_refund_id.as_deref() == Some(provider_refund_id)
            }) {
                r.status = "COMPLETED".to_owned();
                r.updated_at = Utc::now();
            }
        }

        Ok(true)
    }

    async fn create_refund_request(
        &self,
        request: &domain::WalletRefundRequest,
    ) -> WalletRepoResult<Option<domain::WalletRefundRequest>> {
        let mut g = self.refunds.lock().unwrap();
        if let Some(existing) = g.get(&request.idempotency_key) {
            return Ok(Some(existing.clone()));
        }
        g.insert(request.idempotency_key.clone(), request.clone());
        Ok(Some(request.clone()))
    }

    async fn set_refund_provider_id(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
        provider_refund_id: &str,
    ) -> WalletRepoResult<()> {
        let mut g = self.refunds.lock().unwrap();
        if let Some(r) = g.get_mut(idempotency_key)
            && r.tenant_id == tenant_id
            && r.provider_refund_id.is_none()
        {
            r.provider_refund_id = Some(provider_refund_id.to_owned());
            r.status = "SUBMITTED".to_owned();
            r.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn claim_auto_top_up(
        &self,
        tenant_id: Uuid,
        lease_id: Uuid,
        lease_for: std::time::Duration,
    ) -> WalletRepoResult<bool> {
        let mut g = self.claims.lock().unwrap();
        let now = Utc::now();
        if let Some((_, until)) = g.get(&tenant_id)
            && *until > now
        {
            return Ok(false);
        }
        g.insert(
            tenant_id,
            (
                lease_id,
                now + chrono::Duration::from_std(lease_for)
                    .unwrap_or(chrono::Duration::seconds(60)),
            ),
        );
        Ok(true)
    }

    async fn release_auto_top_up(&self, tenant_id: Uuid, lease_id: Uuid) -> WalletRepoResult<()> {
        let mut g = self.claims.lock().unwrap();
        if g.get(&tenant_id).map(|x| x.0) == Some(lease_id) {
            g.remove(&tenant_id);
        }
        Ok(())
    }

    async fn record_auto_top_up_attempt(&self, attempt: &AutoTopUpAttempt) -> WalletRepoResult<()> {
        let mut g = self.attempts.lock().unwrap();
        if !g.iter().any(|x| x.id == attempt.id) {
            g.push(attempt.clone());
        }
        Ok(())
    }

    async fn get_settings(&self, tenant_id: Uuid) -> WalletRepoResult<WalletSettings> {
        let mut g = self.settings.lock().unwrap();
        Ok(g.entry(tenant_id)
            .or_insert_with(|| WalletSettings {
                tenant_id,
                ..WalletSettings::default()
            })
            .clone())
    }

    async fn update_settings(&self, settings: &WalletSettings) -> WalletRepoResult<WalletSettings> {
        if settings.critical_balance_threshold_units > settings.low_balance_threshold_units
            || settings.auto_top_up_amount_units <= 0
            || settings.daily_top_up_limit_units <= 0
            || settings.monthly_top_up_limit_units <= 0
            || !(0..=10).contains(&settings.retry_limit)
        {
            return Err("invalid wallet settings".into());
        }
        self.settings
            .lock()
            .unwrap()
            .insert(settings.tenant_id, settings.clone());
        Ok(settings.clone())
    }

    async fn expire_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<Option<WalletTransaction>> {
        if amount_units <= 0 {
            return Ok(None);
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, txs, _) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        if let Some(x) = txs.iter().find(|t| t.idempotency_key == idempotency_key) {
            return Ok(Some(x.clone()));
        }
        let debit = amount_units.min(wallet.balance_units);
        if debit == 0 {
            return Ok(None);
        }
        wallet.balance_units -= debit;
        wallet.updated_at = Utc::now();
        let tx = WalletTransaction {
            id: Uuid::new_v4(),
            tenant_id,
            wallet_id: wallet.id,
            transaction_type: WalletTransactionType::Expiration,
            amount_units: -debit,
            balance_after_units: wallet.balance_units,
            idempotency_key: idempotency_key.to_owned(),
            reference_type: Some("promotional_credit".to_owned()),
            reference_id: None,
            description: description.map(str::to_owned),
            created_at: Utc::now(),
        };
        txs.push(tx.clone());
        Ok(Some(tx))
    }

    async fn grant_promotional_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        expires_at: DateTime<Utc>,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if expires_at <= Utc::now() {
            return Err("promotional credit already expired".into());
        }
        let tx = self
            .credit(
                tenant_id,
                amount_units,
                WalletTransactionType::PromotionalCredit,
                idempotency_key,
                description,
            )
            .await?;
        let mut promos = self.promos.lock().unwrap();
        if !promos
            .iter()
            .any(|p| p.tenant_id == tenant_id && p.id == tx.id)
        {
            promos.push(Promo {
                id: tx.id,
                tenant_id,
                remaining: amount_units,
                expires_at,
            });
        }
        Ok(tx)
    }

    async fn expire_promotional_credits(&self, limit: u32) -> WalletRepoResult<u64> {
        let now = Utc::now();

        // Extract owned expiration work first. The MutexGuard does not
        // survive this block, so the await below is Send-safe.
        let ids: Vec<(Uuid, Uuid, i64)> = {
            let promos = self.promos.lock().unwrap();

            promos
                .iter()
                .filter(|p| p.expires_at <= now && p.remaining > 0)
                .take(limit.min(500) as usize)
                .map(|p| (p.id, p.tenant_id, p.remaining))
                .collect()
        };

        let mut n = 0u64;

        for (id, tenant, remaining) in ids {
            self.expire_credit(
                tenant,
                remaining,
                &format!("expiration:{id}"),
                Some("Promotional credit expired"),
            )
            .await?;

            {
                let mut promos = self.promos.lock().unwrap();

                if let Some(p) = promos.iter_mut().find(|p| p.id == id) {
                    p.remaining = 0;
                }
            }

            n += 1;
        }

        Ok(n)
    }

    async fn expire_pending_reservations(&self, limit: u32) -> WalletRepoResult<u64> {
        let now = Utc::now();
        let mut g = self.inner.lock().unwrap();
        let mut changed = 0u64;
        for (_, _, reservations) in g.values_mut() {
            for reservation in reservations.values_mut() {
                if changed >= u64::from(limit) {
                    return Ok(changed);
                }
                if reservation.status == CreditReservation::PENDING && reservation.expires_at <= now
                {
                    reservation.status = CreditReservation::EXPIRED.to_owned();
                    reservation.updated_at = now;
                    changed += 1;
                }
            }
        }
        Ok(changed)
    }

    async fn auto_top_up_candidates(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<domain::AutoTopUpCandidate>> {
        let settings = self.settings.lock().unwrap().clone();
        let attempts = self.attempts.lock().unwrap().clone();
        let wallets = self
            .inner
            .lock()
            .unwrap()
            .values()
            .map(|(w, _, _)| w.clone())
            .collect::<Vec<_>>();
        let today = Utc::now().date_naive();
        let month = Utc::now().month();
        let mut out = Vec::new();
        for w in wallets {
            let Some(s) = settings.get(&w.tenant_id) else {
                continue;
            };
            if !s.auto_top_up_enabled || w.balance_units >= s.auto_top_up_threshold_units {
                continue;
            }
            let daily = attempts
                .iter()
                .filter(|a| {
                    a.tenant_id == w.tenant_id
                        && a.attempted_at.date_naive() == today
                        && a.status == "SUCCEEDED"
                })
                .map(|a| a.amount_units)
                .sum();
            let monthly = attempts
                .iter()
                .filter(|a| {
                    a.tenant_id == w.tenant_id
                        && a.attempted_at.month() == month
                        && a.attempted_at.year() == Utc::now().year()
                        && a.status == "SUCCEEDED"
                })
                .map(|a| a.amount_units)
                .sum();
            let daily_attempts = attempts
                .iter()
                .filter(|a| a.tenant_id == w.tenant_id && a.attempted_at.date_naive() == today)
                .count() as i32;
            out.push(domain::AutoTopUpCandidate {
                tenant_id: w.tenant_id,
                balance_units: w.balance_units,
                settings: s.clone(),
                daily_spend_units: daily,
                monthly_spend_units: monthly,
                daily_attempts,
            });
            if out.len() >= limit as usize {
                break;
            }
        }
        Ok(out)
    }

    async fn record_balance_notification(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        dedupe_key: &str,
        message: &str,
    ) -> WalletRepoResult<bool> {
        let mut g = self.notifications.lock().unwrap();
        if g.iter().any(|n| {
            n.tenant_id == tenant_id && n.event_type == event_type && n.dedupe_key == dedupe_key
        }) {
            return Ok(false);
        }
        g.push(WalletNotification {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: event_type.to_owned(),
            dedupe_key: dedupe_key.to_owned(),
            message: message.to_owned(),
            read_at: None,
            email_sent_at: None,
            email_processing_at: None,
            created_at: Utc::now(),
        });
        Ok(true)
    }

    async fn list_balance_notifications(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>> {
        Ok(self
            .notifications
            .lock()
            .unwrap()
            .iter()
            .rev()
            .filter(|n| n.tenant_id == tenant_id)
            .take(limit.min(100) as usize)
            .cloned()
            .collect())
    }

    async fn claim_pending_email_notifications(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>> {
        let mut g = self.notifications.lock().unwrap();
        let now = Utc::now();
        let mut out = Vec::new();
        for n in g
            .iter_mut()
            .filter(|n| n.email_sent_at.is_none() && n.email_processing_at.is_none())
            .take(limit.min(100) as usize)
        {
            n.email_processing_at = Some(now);
            out.push(n.clone());
        }
        Ok(out)
    }

    async fn mark_balance_notification_emailed(&self, id: Uuid) -> WalletRepoResult<bool> {
        let mut g = self.notifications.lock().unwrap();
        if let Some(n) = g
            .iter_mut()
            .find(|n| n.id == id && n.email_sent_at.is_none())
        {
            n.email_sent_at = Some(Utc::now());
            n.email_processing_at = None;
            return Ok(true);
        }
        Ok(false)
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/infra/src/repositories/postgres_payg_pricing_repository.rs")"
cat <<'EOF' > "$PROJECT/crates/infra/src/repositories/postgres_payg_pricing_repository.rs"
use application::ports::{PAYGPricingRepository, PaygPricingRepoResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PAYGPricing;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresPAYGPricingRepository {
    pool: PgPool,
}
impl PostgresPAYGPricingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[derive(FromRow)]
struct Row {
    id: Uuid,
    version: i64,
    operation: String,
    unit_name: String,
    unit_price: i64,
    currency: String,
    effective_from: DateTime<Utc>,
    effective_until: Option<DateTime<Utc>>,
    active: bool,
}
impl Row {
    fn into_domain(self) -> PAYGPricing {
        PAYGPricing {
            id: self.id,
            version: self.version,
            operation: self.operation,
            unit_name: self.unit_name,
            unit_price: self.unit_price,
            currency: self.currency,
            effective_from: self.effective_from,
            effective_until: self.effective_until,
            active: self.active,
        }
    }
}
#[async_trait]
impl PAYGPricingRepository for PostgresPAYGPricingRepository {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let row = sqlx::query_as::<_, Row>(
            "SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active
             FROM payg_pricing_configs
             WHERE operation=$1 AND active=TRUE AND effective_from<=now()
               AND (effective_until IS NULL OR effective_until>now())
             ORDER BY version DESC LIMIT 1",
        ).bind(operation).fetch_optional(&self.pool).await?;
        Ok(row.map(Row::into_domain))
    }
    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
        let rows=sqlx::query_as::<_,Row>("SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active FROM payg_pricing_configs WHERE active=TRUE AND effective_from<=now() AND (effective_until IS NULL OR effective_until>now())
               AND operation IN ('http_check.one_time','tcp_check.one_time','dns_check.one_time','api_flow_check.one_time','proxy_check.one_time','http_check.monitoring','tcp_check.monitoring','dns_check.monitoring','heartbeat_processing','api_flow_check.monitoring','proxy_check.monitoring','rum_event','sms','storage','network_egress')
               AND version=(SELECT MAX(p2.version) FROM payg_pricing_configs p2 WHERE p2.operation=payg_pricing_configs.operation AND p2.active=TRUE AND p2.effective_from<=now() AND (p2.effective_until IS NULL OR p2.effective_until>now())) ORDER BY operation").fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Row::into_domain).collect())
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/infra/src/repositories/postgres_scheduling_repository.rs")"
cat <<'EOF' > "$PROJECT/crates/infra/src/repositories/postgres_scheduling_repository.rs"
use std::{error::Error, time::Duration};

use application::ports::{ClaimedMonitor, MonitorRepository, SchedulingRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::MonitorId;
use sqlx::PgPool;
use uuid::Uuid;

use super::postgres_monitor_repository::PostgresMonitorRepository;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub struct PostgresSchedulingRepository {
    pool: PgPool,
    monitor_repository: PostgresMonitorRepository,
}

#[derive(sqlx::FromRow)]
struct OneTimeRow { id:Uuid, tenant_id:Uuid, monitor_id:Uuid, created_by:Uuid, request_id:String, status:String, execution_id:Uuid, lease_owner:Option<Uuid>, lease_expires_at:Option<DateTime<Utc>>, error:Option<String>, created_at:DateTime<Utc>, started_at:Option<DateTime<Utc>>, completed_at:Option<DateTime<Utc>> }
impl OneTimeRow { fn into_domain(self)->domain::OneTimeCheckRequest { domain::OneTimeCheckRequest{id:self.id,tenant_id:self.tenant_id,monitor_id:self.monitor_id,created_by:self.created_by,request_id:self.request_id,status:self.status,execution_id:self.execution_id,lease_owner:self.lease_owner,lease_expires_at:self.lease_expires_at,error:self.error,created_at:self.created_at,started_at:self.started_at,completed_at:self.completed_at} } }
impl PostgresSchedulingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            monitor_repository: PostgresMonitorRepository::new(pool.clone()),
            pool,
        }
    }
}

#[async_trait]
impl SchedulingRepository for PostgresSchedulingRepository {
    async fn claim_due_monitors(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedMonitor>> {
        let lease_seconds = lease_duration.as_secs() as i64;
        let limit_i64 = i64::from(limit);

        let claimed_rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, Uuid, bool)>(
            r#"
            WITH selected AS (
                SELECT id
                FROM monitors
                WHERE enabled = true
                  AND kind != 'heartbeat'
                  AND lease_expires_at IS NULL
                  AND next_due_at <= $4
                ORDER BY next_due_at
                FOR UPDATE SKIP LOCKED
                LIMIT $1
            )
            UPDATE monitors m
            SET lease_owner = $2,
                lease_expires_at = $4 + ($3::bigint * interval '1 second'),
                lease_claimed_at = COALESCE(m.lease_claimed_at, $4),
                lease_execution_id = COALESCE(m.lease_execution_id, gen_random_uuid())
            FROM selected
            WHERE m.id = selected.id
            RETURNING m.id, m.lease_claimed_at, m.lease_execution_id,
                      (m.interval_seconds >= 300
                       AND (SELECT COUNT(*) FROM monitors m2 WHERE m2.tenant_id=m.tenant_id) <= 10
                       AND COALESCE((SELECT balance_units FROM user_wallets w WHERE w.tenant_id=m.tenant_id LIMIT 1),0) <= 0)
            "#,
        )
        .bind(limit_i64)
        .bind(lease_owner)
        .bind(lease_seconds)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        let mut monitors = Vec::with_capacity(claimed_rows.len());
        for (id, claimed_at, execution_id, free_entitled) in claimed_rows {
            if let Some(monitor) = self.monitor_repository.find_by_id(MonitorId(id)).await? {
                monitors.push(ClaimedMonitor {
                    monitor,
                    claimed_at,
                    execution_id,
                    free_entitled,
                });
            }
        }

        Ok(monitors)
    }

    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE monitors
            SET lease_owner = NULL,
                lease_expires_at = NULL,
                lease_claimed_at = NULL,
                lease_execution_id = NULL,
                next_due_at = now() + (interval_seconds * interval '1 second')
            WHERE id = $1
              AND lease_owner = $2
            "#,
        )
        .bind(monitor_id.0)
        .bind(lease_owner)
        .execute(&self.pool)
        .await?;

        // A zero-row update means this worker no longer owns the claim (for
        // example, the lease expired and another worker reclaimed it). Do not
        // clear or advance another worker's scheduling state.
        if result.rows_affected() == 0 {
            return Err("scheduler claim was lost before completion".into());
        }

        Ok(())
    }

    async fn request_immediate_checks(
        &self,
        tenant_id: domain::shared::ids::TenantId,
    ) -> RepoResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE monitors
            SET next_due_at = LEAST(next_due_at, NOW())
            WHERE tenant_id = $1
              AND enabled = TRUE
              AND kind <> 'heartbeat'
            "#,
        )
        .bind(tenant_id.0)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64> { let result=sqlx::query(r#"UPDATE monitors SET lease_owner=NULL,lease_expires_at=NULL WHERE lease_expires_at IS NOT NULL AND lease_expires_at < $1"#).bind(now).execute(&self.pool).await?; Ok(result.rows_affected()) }
    async fn enqueue_one_time_check(&self,t:domain::shared::ids::TenantId,m:MonitorId,u:domain::shared::ids::UserId,r:&str)->RepoResult<domain::OneTimeCheckRequest>{let row=sqlx::query_as::<_,OneTimeRow>(r#"INSERT INTO one_time_check_requests(id,tenant_id,monitor_id,created_by,request_id,status,execution_id) VALUES($1,$2,$3,$4,$5,'QUEUED',$6) RETURNING id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at"#).bind(Uuid::new_v4()).bind(t.0).bind(m.0).bind(u.0).bind(r).bind(Uuid::new_v4()).fetch_one(&self.pool).await?;Ok(row.into_domain())}
    async fn claim_one_time_checks(&self,l:u32,o:Uuid,d:Duration,n:DateTime<Utc>)->RepoResult<Vec<application::ports::ClaimedOneTimeCheck>>{let sec=d.as_secs() as i64;let rows=sqlx::query_as::<_,OneTimeRow>(r#"WITH selected AS (SELECT id FROM one_time_check_requests WHERE status='QUEUED' OR (status='RUNNING' AND lease_expires_at < $4) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE one_time_check_requests r SET status='RUNNING',lease_owner=$2,lease_expires_at=$4+($3::bigint*interval '1 second'),started_at=COALESCE(started_at,$4) FROM selected WHERE r.id=selected.id RETURNING r.id,r.tenant_id,r.monitor_id,r.created_by,r.request_id,r.status,r.execution_id,r.lease_owner,r.lease_expires_at,r.error,r.created_at,r.started_at,r.completed_at"#).bind(i64::from(l)).bind(o).bind(sec).bind(n).fetch_all(&self.pool).await?;let mut out=Vec::with_capacity(rows.len());for row in rows{if let Some(monitor)=self.monitor_repository.find_by_id(MonitorId(row.monitor_id)).await?{out.push(application::ports::ClaimedOneTimeCheck{request:row.into_domain(),monitor});}}Ok(out)}
    async fn complete_one_time_check(&self,r:Uuid,o:Uuid,s:&str,e:Option<&str>)->RepoResult<()> {if !matches!(s,domain::OneTimeCheckRequest::SUCCEEDED|domain::OneTimeCheckRequest::FAILED){return Err("invalid one-time check terminal status".into())}let n=sqlx::query(r#"UPDATE one_time_check_requests SET status=$3,error=$4,completed_at=now(),lease_owner=NULL,lease_expires_at=NULL WHERE id=$1 AND lease_owner=$2 AND status='RUNNING'"#).bind(r).bind(o).bind(s).bind(e).execute(&self.pool).await?.rows_affected();if n!=1{return Err("one-time check claim was lost before completion".into())}Ok(())}
    async fn get_one_time_check(&self,t:domain::shared::ids::TenantId,m:MonitorId,r:Uuid)->RepoResult<Option<domain::OneTimeCheckRequest>>{let row=sqlx::query_as::<_,OneTimeRow>(r#"SELECT id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at FROM one_time_check_requests WHERE id=$1 AND tenant_id=$2 AND monitor_id=$3"#).bind(r).bind(t.0).bind(m.0).fetch_optional(&self.pool).await?;Ok(row.map(OneTimeRow::into_domain))}
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/infra/src/repositories/postgres_wallet_repository.rs")"
cat <<'EOF' > "$PROJECT/crates/infra/src/repositories/postgres_wallet_repository.rs"
use application::ports::{ReservationOutcome, WalletRepoResult, WalletRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    AutoTopUpAttempt, AutoTopUpCandidate, CreditReservation, Wallet, WalletSettings,
    WalletTopUpSession, WalletTransaction, WalletTransactionType,
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresWalletRepository {
    pool: PgPool,
}
impl PostgresWalletRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn parse_tx_type(v: &str) -> WalletTransactionType {
    match v {
        "TOP_UP" => WalletTransactionType::TopUp,
        "DEBIT" => WalletTransactionType::Debit,
        "REFUND" => WalletTransactionType::Refund,
        "ADJUSTMENT" => WalletTransactionType::Adjustment,
        "PROMOTIONAL_CREDIT" => WalletTransactionType::PromotionalCredit,
        "EXPIRATION" => WalletTransactionType::Expiration,
        _ => WalletTransactionType::Adjustment,
    }
}

#[derive(FromRow)]
struct WalletRow {
    id: Uuid,
    tenant_id: Uuid,
    currency: String,
    balance_units: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl WalletRow {
    fn into_domain(self) -> Wallet {
        Wallet {
            id: self.id,
            tenant_id: self.tenant_id,
            currency: self.currency,
            balance_units: self.balance_units,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(FromRow)]
struct TxRow {
    id: Uuid,
    tenant_id: Uuid,
    wallet_id: Uuid,
    transaction_type: String,
    amount_units: i64,
    balance_after_units: i64,
    idempotency_key: String,
    reference_type: Option<String>,
    reference_id: Option<String>,
    description: Option<String>,
    created_at: DateTime<Utc>,
}
impl TxRow {
    fn into_domain(self) -> WalletTransaction {
        WalletTransaction {
            id: self.id,
            tenant_id: self.tenant_id,
            wallet_id: self.wallet_id,
            transaction_type: parse_tx_type(&self.transaction_type),
            amount_units: self.amount_units,
            balance_after_units: self.balance_after_units,
            idempotency_key: self.idempotency_key,
            reference_type: self.reference_type,
            reference_id: self.reference_id,
            description: self.description,
            created_at: self.created_at,
        }
    }
}

#[derive(FromRow)]
struct ReservationRow {
    id: Uuid,
    wallet_id: Uuid,
    amount_units: i64,
    status: String,
    operation: String,
    request_id: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl ReservationRow {
    fn into_domain(self) -> CreditReservation {
        CreditReservation {
            id: self.id,
            wallet_id: self.wallet_id,
            amount_units: self.amount_units,
            status: self.status,
            operation: self.operation,
            request_id: self.request_id,
            expires_at: self.expires_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

const TX_COLUMNS: &str = "id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description,created_at";

fn validate_usage_for_reservation(
    event: &domain::UsageEvent,
    reservation: &ReservationRow,
    tenant_id: Uuid,
) -> WalletRepoResult<()> {
    if event.tenant_id != tenant_id
        || event.request_id != reservation.request_id
        || event.operation != reservation.operation
        || event.charge_units != reservation.amount_units
        || event.charge_units <= 0
        || event.units_requested <= 0
        || event.units_billed <= 0
        || event.units_billed > event.units_requested
        || event.unit_price <= 0
        || event.status != "CAPTURED"
        || event.charge_units
            != event
                .units_billed
                .checked_mul(event.unit_price)
                .ok_or("usage charge overflow")?
    {
        return Err("usage event does not match PAYG reservation".into());
    }
    Ok(())
}

#[async_trait]
impl WalletRepository for PostgresWalletRepository {
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        // This endpoint is read-heavy. `FOR UPDATE` here needlessly serialized
        // concurrent dashboard reads against the wallet row. The unique
        // tenant constraint makes the insert race-safe; the following plain
        // SELECT is sufficient because callers are not mutating the wallet.
        sqlx::query(
            "INSERT INTO user_wallets(id,tenant_id,currency) VALUES($1,$2,$3)
             ON CONFLICT(tenant_id) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(currency)
        .execute(&self.pool)
        .await?;
        let row = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into_domain())
    }

    async fn get_balance_summary(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<domain::WalletBalanceSummary> {
        let balance: i64 = sqlx::query_scalar("SELECT balance_units FROM user_wallets WHERE tenant_id=$1 AND currency=$2")
            .bind(tenant_id).bind(currency).fetch_optional(&self.pool).await?.ok_or("wallet not found")?;
        let reserved: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(amount_units),0) FROM credit_reservations WHERE wallet_id=(SELECT id FROM user_wallets WHERE tenant_id=$1 AND currency=$2) AND status='PENDING' AND expires_at>now()")
            .bind(tenant_id).bind(currency).fetch_one(&self.pool).await?;
        Ok(domain::WalletBalanceSummary{balance_units:balance,reserved_units:reserved,available_units:balance.saturating_sub(reserved)})
    }

    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>> {
        let query = format!(
            "SELECT t.{TX_COLUMNS} FROM wallet_transactions t
             WHERE t.tenant_id=$1 ORDER BY t.created_at DESC LIMIT $2"
        );
        let rows = sqlx::query_as::<_, TxRow>(&query)
            .bind(tenant_id)
            .bind(limit.clamp(1, 100) as i64)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(TxRow::into_domain).collect())
    }

    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome> {
        if amount_units <= 0 {
            return Err("reservation amount must be positive".into());
        }
        if request_id.trim().is_empty() {
            return Err("request id is required".into());
        }
        let mut tx = self.pool.begin().await?;
        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;

        if let Some(existing) = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at
             FROM credit_reservations WHERE wallet_id=$1 AND request_id=$2",
        ).bind(wallet.id).bind(request_id).fetch_optional(&mut *tx).await? {
            if (existing.status == CreditReservation::PENDING && existing.expires_at <= Utc::now())
                || existing.status == CreditReservation::EXPIRED
            {
                let refreshed = sqlx::query_as::<_, ReservationRow>(
                    "UPDATE credit_reservations
                     SET amount_units=$3, operation=$4, expires_at=$5, status='PENDING', updated_at=now()
                     WHERE wallet_id=$1 AND request_id=$2
                       AND (status='EXPIRED' OR (status='PENDING' AND expires_at<=now()))
                     RETURNING id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at",
                )
                .bind(wallet.id)
                .bind(request_id)
                .bind(amount_units)
                .bind(operation)
                .bind(expires_at)
                .fetch_optional(&mut *tx)
                .await?;
                if let Some(refreshed) = refreshed {
                    tx.commit().await?;
                    return Ok(ReservationOutcome { reservation: refreshed.into_domain(), created: true });
                }
            }

            tx.commit().await?;
            return Ok(ReservationOutcome { reservation: existing.into_domain(), created: false });
        }

        let reserved_units: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_units),0)::BIGINT
             FROM credit_reservations
             WHERE wallet_id=$1 AND status='PENDING' AND expires_at>now()",
        )
        .bind(wallet.id)
        .fetch_one(&mut *tx)
        .await?;
        let available = wallet
            .balance_units
            .checked_sub(reserved_units)
            .ok_or("wallet reservation accounting overflow")?;
        if available < amount_units {
            // Persist the billing block in the same wallet transaction. This is
            // durable state, not merely a UI-derived balance label, so every
            // worker/region observes the same decision.
            sqlx::query(
                "UPDATE user_wallets
                 SET billing_status='PAYG_BALANCE_EXHAUSTED', updated_at=now()
                 WHERE id=$1",
            )
            .bind(wallet.id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Err("insufficient wallet balance".into());
        }

        // A successful authorization is also the durable resume transition.
        sqlx::query(
            "UPDATE user_wallets
             SET billing_status='ACTIVE', updated_at=now()
             WHERE id=$1",
        )
        .bind(wallet.id)
        .execute(&mut *tx)
        .await?;

        let row = sqlx::query_as::<_, ReservationRow>(
            "INSERT INTO credit_reservations
             (id,wallet_id,amount_units,status,operation,request_id,expires_at)
             VALUES($1,$2,$3,'PENDING',$4,$5,$6)
             RETURNING id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at",
        ).bind(Uuid::new_v4()).bind(wallet.id).bind(amount_units).bind(operation)
         .bind(request_id).bind(expires_at).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(ReservationOutcome {
            reservation: row.into_domain(),
            created: true,
        })
    }

    async fn capture(
        &self,
        reservation_id: Uuid,
        usage_event: Option<&domain::UsageEvent>,
        audit_entries: &[domain::FinancialAuditLogEntry],
    ) -> WalletRepoResult<WalletTransaction> {
        let mut tx = self.pool.begin().await?;
        let r = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at
             FROM credit_reservations WHERE id=$1 FOR UPDATE",
        ).bind(reservation_id).fetch_optional(&mut *tx).await?
            .ok_or("reservation not found")?;

        let idem = format!("reservation:{reservation_id}");
        if r.status == CreditReservation::CAPTURED {
            let q =
                format!("SELECT {TX_COLUMNS} FROM wallet_transactions WHERE idempotency_key=$1");
            let row = sqlx::query_as::<_, TxRow>(&q)
                .bind(&idem)
                .fetch_one(&mut *tx)
                .await?;
            if let Some(event) = usage_event {
                validate_usage_for_reservation(event, &r, row.tenant_id)?;
                sqlx::query(
                    "INSERT INTO api_usage_events
                     (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,pricing_version,request_id,status)
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                     ON CONFLICT (tenant_id,request_id) DO NOTHING",
                )
                .bind(event.id).bind(event.tenant_id).bind(&event.operation)
                .bind(event.units_requested).bind(event.units_billed).bind(event.unit_price)
                .bind(event.charge_units).bind(&event.currency).bind(event.pricing_version)
                .bind(&event.request_id).bind(&event.status)
                .execute(&mut *tx).await?;
            }
            for audit in audit_entries {
                sqlx::query(
                    "INSERT INTO financial_audit_log(id,actor,tenant_id,action,resource,resource_id,created_at,metadata)
                     SELECT $1,$2,$3,$4,$5,$6,$7,$8
                     WHERE NOT EXISTS (
                         SELECT 1 FROM financial_audit_log
                         WHERE action=$4 AND resource=$5 AND resource_id=$6
                     )",
                )
                .bind(audit.id).bind(&audit.actor).bind(audit.tenant_id).bind(&audit.action)
                .bind(&audit.resource).bind(&audit.resource_id).bind(audit.created_at).bind(&audit.metadata)
                .execute(&mut *tx).await?;
            }
            tx.commit().await?;
            return Ok(row.into_domain());
        }
        if r.status != CreditReservation::PENDING || r.expires_at <= Utc::now() {
            return Err("reservation is not capturable".into());
        }

        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE id=$1 FOR UPDATE",
        )
        .bind(r.wallet_id)
        .fetch_one(&mut *tx)
        .await?;
        let new_balance = wallet
            .balance_units
            .checked_sub(r.amount_units)
            .ok_or("insufficient wallet balance")?;

        // Promotional credits are part of the wallet balance, but they expire.
        // Consume the oldest active promotional credits first so expiration can
        // never claw back cash that was already purchased.
        let mut promo_remaining = r.amount_units;
        let promo_rows = sqlx::query_as::<_, (Uuid, i64)>(
            "SELECT id, remaining_amount_units
             FROM promotional_credits
             WHERE wallet_id=$1 AND status='ACTIVE' AND remaining_amount_units>0
               AND expires_at>now()
             ORDER BY expires_at ASC, created_at ASC
             FOR UPDATE",
        )
        .bind(wallet.id)
        .fetch_all(&mut *tx)
        .await?;
        for (promo_id, remaining) in promo_rows {
            if promo_remaining == 0 {
                break;
            }
            let used = remaining.min(promo_remaining);
            let left = remaining - used;
            sqlx::query(
                "UPDATE promotional_credits
                 SET remaining_amount_units=$2,
                     status=CASE WHEN $2=0 THEN 'EXPIRED' ELSE status END,
                     updated_at=now()
                 WHERE id=$1",
            )
            .bind(promo_id)
            .bind(left)
            .execute(&mut *tx)
            .await?;
            promo_remaining -= used;
        }

        sqlx::query(
            "UPDATE user_wallets
             SET balance_units=$2,
                 billing_status=CASE WHEN $2 <= 0 THEN 'PAYG_BALANCE_EXHAUSTED' ELSE 'ACTIVE' END,
                 updated_at=now()
             WHERE id=$1",
        )
        .bind(wallet.id)
        .bind(new_balance)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE credit_reservations SET status='CAPTURED',updated_at=now() WHERE id=$1",
        )
        .bind(r.id)
        .execute(&mut *tx)
        .await?;

        let q = format!(
            "INSERT INTO wallet_transactions
             (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description)
             VALUES($1,$2,$3,'DEBIT',$4,$5,$6,'credit_reservation',$7,$8)
             RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(wallet.tenant_id)
            .bind(wallet.id)
            .bind(-r.amount_units)
            .bind(new_balance)
            .bind(&idem)
            .bind(r.id.to_string())
            .bind(r.operation.clone())
            .fetch_one(&mut *tx)
            .await?;
        if let Some(event) = usage_event {
            validate_usage_for_reservation(event, &r, wallet.tenant_id)?;
            sqlx::query(
                "INSERT INTO api_usage_events
                 (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,pricing_version,request_id,status)
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                 ON CONFLICT (tenant_id,request_id) DO NOTHING",
            )
            .bind(event.id).bind(event.tenant_id).bind(&event.operation)
            .bind(event.units_requested).bind(event.units_billed).bind(event.unit_price)
            .bind(event.charge_units).bind(&event.currency).bind(event.pricing_version)
            .bind(&event.request_id).bind(&event.status)
            .execute(&mut *tx).await?;
        }
        for audit in audit_entries {
            sqlx::query(
                "INSERT INTO financial_audit_log(id,actor,tenant_id,action,resource,resource_id,created_at,metadata)
                 SELECT $1,$2,$3,$4,$5,$6,$7,$8
                 WHERE NOT EXISTS (
                     SELECT 1 FROM financial_audit_log
                     WHERE action=$4 AND resource=$5 AND resource_id=$6
                 )",
            )
            .bind(audit.id).bind(&audit.actor).bind(audit.tenant_id).bind(&audit.action)
            .bind(&audit.resource).bind(&audit.resource_id).bind(audit.created_at).bind(&audit.metadata)
            .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        let domain_row = row.into_domain();

        // Record threshold transitions close to the actual debit, not only when
        // a user opens the wallet page. This makes zero/critical/low alerts
        // durable even when usage is generated by background jobs.
        let Ok(settings) = self.get_settings(wallet.tenant_id).await else {
            return Ok(domain_row);
        };
        let level = if new_balance <= 0 {
            "BLOCKED"
        } else if new_balance <= settings.critical_balance_threshold_units {
            "CRITICAL"
        } else if new_balance <= settings.low_balance_threshold_units {
            "WARNING"
        } else {
            "OK"
        };
        if level != "OK" {
            let _ = self
                .record_balance_notification(
                    wallet.tenant_id,
                    level,
                    &format!("{}:{}", level, Utc::now().date_naive()),
                    match level {
                        "BLOCKED" => "PAYG is blocked because your wallet balance is zero.",
                        "CRITICAL" => "Your PAYG wallet balance is critically low.",
                        _ => "Your PAYG wallet balance is low.",
                    },
                )
                .await;
        }
        Ok(domain_row)
    }

    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()> {
        sqlx::query(
            "UPDATE credit_reservations SET status='RELEASED',updated_at=now()
             WHERE id=$1 AND status='PENDING'",
        )
        .bind(reservation_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if amount_units <= 0 {
            return Err("credit amount must be positive".into());
        }
        if idempotency_key.trim().is_empty() {
            return Err("idempotency key is required".into());
        }
        let mut tx = self.pool.begin().await?;
        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;
        let q = format!(
            "SELECT {TX_COLUMNS} FROM wallet_transactions WHERE tenant_id=$1 AND idempotency_key=$2"
        );
        if let Some(row) = sqlx::query_as::<_, TxRow>(&q)
            .bind(tenant_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *tx)
            .await?
        {
            tx.commit().await?;
            return Ok(row.into_domain());
        }
        let new_balance = wallet
            .balance_units
            .checked_add(amount_units)
            .ok_or("wallet balance overflow")?;
        sqlx::query(
            "UPDATE user_wallets
             SET balance_units=$2,
                 billing_status='ACTIVE',
                 updated_at=now()
             WHERE id=$1",
        )
        .bind(wallet.id)
        .bind(new_balance)
        .execute(&mut *tx)
        .await?;
        let q = format!(
            "INSERT INTO wallet_transactions
             (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,description)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(wallet.id)
            .bind(transaction_type.as_str())
            .bind(amount_units)
            .bind(new_balance)
            .bind(idempotency_key)
            .bind(description)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }
    async fn get_settings(&self, tenant_id: Uuid) -> WalletRepoResult<WalletSettings> {
        #[derive(sqlx::FromRow)]
        struct SettingsRow {
            low_balance_threshold_units: i64,
            critical_balance_threshold_units: i64,
            auto_top_up_enabled: bool,
            auto_top_up_threshold_units: i64,
            auto_top_up_amount_units: i64,
            payment_method: Option<String>,
            daily_top_up_limit_units: i64,
            monthly_top_up_limit_units: i64,
            retry_limit: i32,
        }
        let r = sqlx::query_as::<_, SettingsRow>("SELECT low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,retry_limit FROM wallet_settings WHERE tenant_id=$1").bind(tenant_id).fetch_optional(&self.pool).await?;
        Ok(r.map(|x| WalletSettings {
            tenant_id,
            low_balance_threshold_units: x.low_balance_threshold_units,
            critical_balance_threshold_units: x.critical_balance_threshold_units,
            auto_top_up_enabled: x.auto_top_up_enabled,
            auto_top_up_threshold_units: x.auto_top_up_threshold_units,
            auto_top_up_amount_units: x.auto_top_up_amount_units,
            payment_method: x.payment_method,
            daily_top_up_limit_units: x.daily_top_up_limit_units,
            monthly_top_up_limit_units: x.monthly_top_up_limit_units,
            retry_limit: x.retry_limit,
        })
        .unwrap_or_else(|| WalletSettings {
            tenant_id,
            ..WalletSettings::default()
        }))
    }
    async fn update_settings(&self, settings: &WalletSettings) -> WalletRepoResult<WalletSettings> {
        if settings.low_balance_threshold_units < 0
            || settings.critical_balance_threshold_units < 0
            || settings.auto_top_up_threshold_units < 0
            || settings.auto_top_up_amount_units <= 0
            || settings.daily_top_up_limit_units <= 0
            || settings.monthly_top_up_limit_units <= 0
            || settings.retry_limit < 0
            || settings.critical_balance_threshold_units > settings.low_balance_threshold_units
        {
            return Err("invalid wallet settings".into());
        }
        sqlx::query("INSERT INTO wallet_settings (tenant_id,low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,retry_limit) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(tenant_id) DO UPDATE SET low_balance_threshold_units=EXCLUDED.low_balance_threshold_units,critical_balance_threshold_units=EXCLUDED.critical_balance_threshold_units,auto_top_up_enabled=EXCLUDED.auto_top_up_enabled,auto_top_up_threshold_units=EXCLUDED.auto_top_up_threshold_units,auto_top_up_amount_units=EXCLUDED.auto_top_up_amount_units,payment_method=EXCLUDED.payment_method,daily_top_up_limit_units=EXCLUDED.daily_top_up_limit_units,monthly_top_up_limit_units=EXCLUDED.monthly_top_up_limit_units,retry_limit=EXCLUDED.retry_limit,updated_at=now()").bind(settings.tenant_id).bind(settings.low_balance_threshold_units).bind(settings.critical_balance_threshold_units).bind(settings.auto_top_up_enabled).bind(settings.auto_top_up_threshold_units).bind(settings.auto_top_up_amount_units).bind(&settings.payment_method).bind(settings.daily_top_up_limit_units).bind(settings.monthly_top_up_limit_units).bind(settings.retry_limit).execute(&self.pool).await?;
        Ok(settings.clone())
    }
    async fn expire_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<Option<WalletTransaction>> {
        if amount_units <= 0 || idempotency_key.trim().is_empty() {
            return Ok(None);
        }
        let mut tx = self.pool.begin().await?;
        let wallet=sqlx::query_as::<_,WalletRow>("SELECT id,tenant_id,currency,balance_units,created_at,updated_at FROM user_wallets WHERE tenant_id=$1 FOR UPDATE").bind(tenant_id).fetch_one(&mut *tx).await?;
        let q = format!(
            "SELECT {TX_COLUMNS} FROM wallet_transactions WHERE tenant_id=$1 AND idempotency_key=$2"
        );
        if let Some(row) = sqlx::query_as::<_, TxRow>(&q)
            .bind(tenant_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *tx)
            .await?
        {
            tx.commit().await?;
            return Ok(Some(row.into_domain()));
        }
        let debit = amount_units.min(wallet.balance_units);
        if debit == 0 {
            tx.commit().await?;
            return Ok(None);
        }
        let new_balance = wallet.balance_units - debit;
        sqlx::query("UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1")
            .bind(wallet.id)
            .bind(new_balance)
            .execute(&mut *tx)
            .await?;
        let q = format!(
            "INSERT INTO wallet_transactions (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,description) VALUES($1,$2,$3,'EXPIRATION',$4,$5,$6,$7) RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(wallet.id)
            .bind(-debit)
            .bind(new_balance)
            .bind(idempotency_key)
            .bind(description)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(row.into_domain()))
    }

    async fn grant_promotional_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        expires_at: DateTime<Utc>,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if amount_units <= 0 || expires_at <= Utc::now() || idempotency_key.trim().is_empty() {
            return Err("invalid promotional credit".into());
        }
        let mut tx = self.pool.begin().await?;
        let wallet=sqlx::query_as::<_,WalletRow>("SELECT id,tenant_id,currency,balance_units,created_at,updated_at FROM user_wallets WHERE tenant_id=$1 FOR UPDATE").bind(tenant_id).fetch_one(&mut *tx).await?;
        let q = format!(
            "SELECT {TX_COLUMNS} FROM wallet_transactions WHERE tenant_id=$1 AND idempotency_key=$2"
        );
        if let Some(row) = sqlx::query_as::<_, TxRow>(&q)
            .bind(tenant_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *tx)
            .await?
        {
            tx.commit().await?;
            return Ok(row.into_domain());
        }
        let new_balance = wallet
            .balance_units
            .checked_add(amount_units)
            .ok_or("wallet balance overflow")?;
        sqlx::query("UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1")
            .bind(wallet.id)
            .bind(new_balance)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO promotional_credits(id,tenant_id,wallet_id,original_amount_units,remaining_amount_units,expires_at,status,idempotency_key,created_at,updated_at) VALUES($1,$2,$3,$4,$4,$5,'ACTIVE',$6,now(),now())").bind(Uuid::new_v4()).bind(tenant_id).bind(wallet.id).bind(amount_units).bind(expires_at).bind(idempotency_key).execute(&mut *tx).await?;
        let q = format!(
            "INSERT INTO wallet_transactions (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,description) VALUES($1,$2,$3,'PROMOTIONAL_CREDIT',$4,$5,$6,$7) RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(wallet.id)
            .bind(amount_units)
            .bind(new_balance)
            .bind(idempotency_key)
            .bind(description)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }
    async fn expire_promotional_credits(&self, limit: u32) -> WalletRepoResult<u64> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, (Uuid, Uuid)>(
            "SELECT id,tenant_id
             FROM promotional_credits
             WHERE status='ACTIVE' AND expires_at<=now() AND remaining_amount_units>0
             ORDER BY expires_at ASC
             LIMIT $1",
        )
        .bind(limit.clamp(1, 500) as i64)
        .fetch_all(&mut *tx)
        .await?;

        let mut count = 0_u64;
        for (promo_id, tenant_id) in rows {
            // Lock order is wallet -> promo, matching capture(), to avoid
            // wallet/promo deadlocks under concurrent expiry and usage.
            let wallet = sqlx::query_as::<_, WalletRow>(
                "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
                 FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
            )
            .bind(tenant_id)
            .fetch_one(&mut *tx)
            .await?;
            let remaining: i64 = sqlx::query_scalar(
                "SELECT remaining_amount_units FROM promotional_credits
                 WHERE id=$1 AND status='ACTIVE' AND expires_at<=now()
                 FOR UPDATE",
            )
            .bind(promo_id)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(0);
            if remaining <= 0 {
                continue;
            }
            let debit = remaining.min(wallet.balance_units);
            if debit > 0 {
                let new_balance = wallet.balance_units - debit;
                sqlx::query(
                    "UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1",
                )
                .bind(wallet.id)
                .bind(new_balance)
                .execute(&mut *tx)
                .await?;
                let idem = format!("expiration:{promo_id}");
                let q = format!(
                    "INSERT INTO wallet_transactions
                     (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description)
                     VALUES($1,$2,$3,'EXPIRATION',$4,$5,$6,'promotional_credit',$7,$8)
                     ON CONFLICT (tenant_id,idempotency_key) DO NOTHING
                     RETURNING {TX_COLUMNS}"
                );
                let _ = sqlx::query_as::<_, TxRow>(&q)
                    .bind(Uuid::new_v4())
                    .bind(tenant_id)
                    .bind(wallet.id)
                    .bind(-debit)
                    .bind(new_balance)
                    .bind(&idem)
                    .bind(promo_id.to_string())
                    .bind("Promotional credit expired")
                    .fetch_optional(&mut *tx)
                    .await?;
            }
            sqlx::query(
                "UPDATE promotional_credits
                 SET remaining_amount_units=0,status='EXPIRED',updated_at=now()
                 WHERE id=$1 AND status='ACTIVE'",
            )
            .bind(promo_id)
            .execute(&mut *tx)
            .await?;
            count += 1;
        }
        tx.commit().await?;
        Ok(count)
    }

    async fn expire_pending_reservations(&self, limit: u32) -> WalletRepoResult<u64> {
        let result = sqlx::query(
            "UPDATE credit_reservations
             SET status='EXPIRED', updated_at=now()
             WHERE id IN (
                 SELECT id FROM credit_reservations
                 WHERE status='PENDING' AND expires_at <= now()
                 ORDER BY expires_at
                 LIMIT $1
                 FOR UPDATE SKIP LOCKED
             )",
        )
        .bind(i64::from(limit.min(10_000)))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn auto_top_up_candidates(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<AutoTopUpCandidate>> {
        #[derive(sqlx::FromRow)]
        struct AutoTopUpRow {
            tenant_id: Uuid,
            balance_units: i64,
            low: i64,
            critical: i64,
            enabled: bool,
            threshold: i64,
            amount: i64,
            payment_method: Option<String>,
            daily_limit: i64,
            monthly_limit: i64,
            retry_limit: i32,
            daily: i64,
            monthly: i64,
            attempts: i64,
        }
        let rows = sqlx::query_as::<_, AutoTopUpRow>(
            "SELECT w.tenant_id,w.balance_units,
                    s.low_balance_threshold_units low,
                    s.critical_balance_threshold_units critical,
                    s.auto_top_up_enabled enabled,
                    s.auto_top_up_threshold_units threshold,
                    s.auto_top_up_amount_units amount,
                    s.payment_method,
                    s.daily_top_up_limit_units daily_limit,
                    s.monthly_top_up_limit_units monthly_limit,
                    s.retry_limit,
                    COALESCE((SELECT SUM(amount_units) FROM wallet_auto_top_up_attempts a
                              WHERE a.tenant_id=w.tenant_id AND a.status='SUCCEEDED'
                                AND a.attempted_at>=date_trunc('day',now())),0) daily,
                    COALESCE((SELECT SUM(amount_units) FROM wallet_auto_top_up_attempts a
                              WHERE a.tenant_id=w.tenant_id AND a.status='SUCCEEDED'
                                AND a.attempted_at>=date_trunc('month',now())),0) monthly,
                    COALESCE((SELECT COUNT(*) FROM wallet_auto_top_up_attempts a
                              WHERE a.tenant_id=w.tenant_id AND a.attempted_at>=date_trunc('day',now())),0) attempts
             FROM user_wallets w
             JOIN wallet_settings s ON s.tenant_id=w.tenant_id
             WHERE s.auto_top_up_enabled
               AND w.balance_units < s.auto_top_up_threshold_units
             ORDER BY w.balance_units
             LIMIT $1"
        )
        .bind(limit.clamp(1,100) as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| domain::AutoTopUpCandidate {
                tenant_id: r.tenant_id,
                balance_units: r.balance_units,
                settings: WalletSettings {
                    tenant_id: r.tenant_id,
                    low_balance_threshold_units: r.low,
                    critical_balance_threshold_units: r.critical,
                    auto_top_up_enabled: r.enabled,
                    auto_top_up_threshold_units: r.threshold,
                    auto_top_up_amount_units: r.amount,
                    payment_method: r.payment_method,
                    daily_top_up_limit_units: r.daily_limit,
                    monthly_top_up_limit_units: r.monthly_limit,
                    retry_limit: r.retry_limit,
                },
                daily_spend_units: r.daily,
                monthly_spend_units: r.monthly,
                daily_attempts: r.attempts as i32,
            })
            .collect())
    }

    async fn create_top_up_session(&self, session: &WalletTopUpSession) -> WalletRepoResult<()> {
        sqlx::query(
            "INSERT INTO wallet_top_up_sessions
             (id,tenant_id,provider,provider_checkout_id,provider_payment_id,amount_units,refunded_amount_units,currency,status)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        )
        .bind(session.id).bind(session.tenant_id).bind(&session.provider)
        .bind(&session.provider_checkout_id).bind(&session.provider_payment_id)
        .bind(session.amount_units).bind(session.refunded_amount_units).bind(&session.currency).bind(&session.status)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow1 {
            id: Uuid,
            tenant_id: Uuid,
            provider: String,
            provider_checkout_id: String,
            provider_payment_id: Option<String>,
            amount_units: i64,
            refunded_amount_units: i64,
            currency: String,
            status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }
        let r = sqlx::query_as::<_,WalletRepoRow1>(
            "SELECT id,tenant_id,provider,provider_checkout_id,provider_payment_id,amount_units,refunded_amount_units,currency,status,created_at,updated_at
             FROM wallet_top_up_sessions WHERE tenant_id=$1 AND provider_checkout_id=$2"
        ).bind(tenant_id).bind(provider_checkout_id).fetch_optional(&self.pool).await?;
        Ok(r.map(|x| WalletTopUpSession {
            id: x.id,
            tenant_id: x.tenant_id,
            provider: x.provider,
            provider_checkout_id: x.provider_checkout_id,
            provider_payment_id: x.provider_payment_id,
            amount_units: x.amount_units,
            refunded_amount_units: x.refunded_amount_units,
            currency: x.currency,
            status: x.status,
            created_at: x.created_at,
            updated_at: x.updated_at,
        }))
    }

    async fn complete_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        provider_payment_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        let mut tx = self.pool.begin().await?;
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow2 {
            id: Uuid,
            tenant_id: Uuid,
            provider: String,
            provider_checkout_id: String,
            provider_payment_id: Option<String>,
            amount_units: i64,
            refunded_amount_units: i64,
            currency: String,
            status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }
        let r=sqlx::query_as::<_,WalletRepoRow2>(
            "SELECT id,tenant_id,provider,provider_checkout_id,provider_payment_id,amount_units,refunded_amount_units,currency,status,created_at,updated_at
             FROM wallet_top_up_sessions WHERE tenant_id=$1 AND provider_checkout_id=$2 FOR UPDATE"
        ).bind(tenant_id).bind(provider_checkout_id).fetch_optional(&mut *tx).await?;
        let Some(r) = r else {
            tx.commit().await?;
            return Ok(None);
        };
        if r.status == WalletTopUpSession::COMPLETED {
            tx.commit().await?;
            return Ok(Some(WalletTopUpSession {
                id: r.id,
                tenant_id: r.tenant_id,
                provider: r.provider,
                provider_checkout_id: r.provider_checkout_id,
                provider_payment_id: r.provider_payment_id,
                amount_units: r.amount_units,
                refunded_amount_units: r.refunded_amount_units,
                currency: r.currency,
                status: r.status,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }));
        }
        sqlx::query("UPDATE wallet_top_up_sessions SET status='COMPLETED',provider_payment_id=$3,updated_at=now() WHERE tenant_id=$1 AND provider_checkout_id=$2")
            .bind(tenant_id).bind(provider_checkout_id).bind(provider_payment_id).execute(&mut *tx).await?;
        tx.commit().await?;
        self.find_top_up_session(tenant_id, provider_checkout_id)
            .await
    }

    async fn refundable_top_up(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow3 {
            id: Uuid,
            tenant_id: Uuid,
            provider: String,
            provider_checkout_id: String,
            provider_payment_id: Option<String>,
            amount_units: i64,
            refunded_amount_units: i64,
            currency: String,
            status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }
        let r=sqlx::query_as::<_,WalletRepoRow3>(
            "SELECT id,tenant_id,provider,provider_checkout_id,provider_payment_id,amount_units,refunded_amount_units,currency,status,created_at,updated_at
             FROM wallet_top_up_sessions
             WHERE tenant_id=$1 AND provider_payment_id=$2 AND status='COMPLETED' AND (amount_units-refunded_amount_units)>=$3
             LIMIT 1"
        ).bind(tenant_id).bind(provider_payment_id).bind(amount_units).fetch_optional(&self.pool).await?;
        Ok(r.map(|x| WalletTopUpSession {
            id: x.id,
            tenant_id: x.tenant_id,
            provider: x.provider,
            provider_checkout_id: x.provider_checkout_id,
            provider_payment_id: x.provider_payment_id,
            amount_units: x.amount_units,
            refunded_amount_units: x.refunded_amount_units,
            currency: x.currency,
            status: x.status,
            created_at: x.created_at,
            updated_at: x.updated_at,
        }))
    }

    async fn record_verified_refund(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
        provider_refund_id: &str,
    ) -> WalletRepoResult<bool> {
        if amount_units <= 0 || provider_refund_id.trim().is_empty() {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        let already_credited: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM wallet_transactions
                WHERE tenant_id=$1 AND reference_type='provider_refund' AND reference_id=$2
            )",
        )
        .bind(tenant_id)
        .bind(provider_refund_id)
        .fetch_one(&mut *tx)
        .await?;
        if already_credited {
            tx.commit().await?;
            return Ok(true);
        }
        let session = sqlx::query_as::<_, (Uuid, i64, i64)>(
            "SELECT id,amount_units,refunded_amount_units
             FROM wallet_top_up_sessions
             WHERE tenant_id=$1 AND provider_payment_id=$2 AND status='COMPLETED'
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(provider_payment_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((session_id, original, refunded)) = session else {
            tx.commit().await?;
            return Ok(false);
        };
        if refunded.saturating_add(amount_units) > original {
            return Err("refund exceeds remaining refundable payment amount".into());
        }

        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;

        let new_balance = wallet
            .balance_units
            .checked_add(amount_units)
            .ok_or("wallet balance overflow")?;

        sqlx::query(
            "UPDATE wallet_top_up_sessions
             SET refunded_amount_units=refunded_amount_units+$3,updated_at=now()
             WHERE id=$1 AND tenant_id=$2",
        )
        .bind(session_id)
        .bind(tenant_id)
        .bind(amount_units)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1")
            .bind(wallet.id)
            .bind(new_balance)
            .execute(&mut *tx)
            .await?;

        let idem = format!("refund:{provider_refund_id}");
        let q = format!(
            "INSERT INTO wallet_transactions
             (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description)
             VALUES($1,$2,$3,'REFUND',$4,$5,$6,'provider_refund',$7,$8)
             ON CONFLICT (tenant_id,idempotency_key) DO NOTHING
             RETURNING {TX_COLUMNS}"
        );
        let _ = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(wallet.id)
            .bind(amount_units)
            .bind(new_balance)
            .bind(&idem)
            .bind(provider_refund_id)
            .bind("Verified payment refund")
            .fetch_optional(&mut *tx)
            .await?;

        sqlx::query("UPDATE wallet_refund_requests SET status='COMPLETED',updated_at=now() WHERE tenant_id=$1 AND provider_refund_id=$2")
            .bind(tenant_id).bind(provider_refund_id).execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(true)
    }

    async fn create_refund_request(
        &self,
        request: &domain::WalletRefundRequest,
    ) -> WalletRepoResult<Option<domain::WalletRefundRequest>> {
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow4 {
            id: Uuid,
            tenant_id: Uuid,
            provider_payment_id: String,
            amount_units: i64,
            idempotency_key: String,
            provider_refund_id: Option<String>,
            status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }
        let row=sqlx::query_as::<_,WalletRepoRow4>(
            "INSERT INTO wallet_refund_requests
             (id,tenant_id,provider_payment_id,amount_units,idempotency_key,status)
             VALUES($1,$2,$3,$4,$5,'PENDING')
             ON CONFLICT (tenant_id,idempotency_key) DO NOTHING
             RETURNING id,tenant_id,provider_payment_id,amount_units,idempotency_key,provider_refund_id,status,created_at,updated_at"
        ).bind(request.id).bind(request.tenant_id).bind(&request.provider_payment_id)
         .bind(request.amount_units).bind(&request.idempotency_key)
         .fetch_optional(&self.pool).await?;
        if let Some(r) = row {
            return Ok(Some(domain::WalletRefundRequest {
                id: r.id,
                tenant_id: r.tenant_id,
                provider_payment_id: r.provider_payment_id,
                amount_units: r.amount_units,
                idempotency_key: r.idempotency_key,
                provider_refund_id: r.provider_refund_id,
                status: r.status,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }));
        }
        let r=sqlx::query_as::<_,WalletRepoRow4>(
            "SELECT id,tenant_id,provider_payment_id,amount_units,idempotency_key,provider_refund_id,status,created_at,updated_at
             FROM wallet_refund_requests WHERE tenant_id=$1 AND idempotency_key=$2"
        ).bind(request.tenant_id).bind(&request.idempotency_key).fetch_optional(&self.pool).await?;
        Ok(r.map(|r| domain::WalletRefundRequest {
            id: r.id,
            tenant_id: r.tenant_id,
            provider_payment_id: r.provider_payment_id,
            amount_units: r.amount_units,
            idempotency_key: r.idempotency_key,
            provider_refund_id: r.provider_refund_id,
            status: r.status,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn set_refund_provider_id(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
        provider_refund_id: &str,
    ) -> WalletRepoResult<()> {
        sqlx::query("UPDATE wallet_refund_requests SET provider_refund_id=$3,status='SUBMITTED',updated_at=now() WHERE tenant_id=$1 AND idempotency_key=$2 AND provider_refund_id IS NULL")
            .bind(tenant_id).bind(idempotency_key).bind(provider_refund_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn claim_auto_top_up(
        &self,
        tenant_id: Uuid,
        lease_id: Uuid,
        lease_for: std::time::Duration,
    ) -> WalletRepoResult<bool> {
        let seconds = lease_for.as_secs().clamp(30, 900) as i64;
        let r = sqlx::query(
            "INSERT INTO wallet_auto_top_up_claims(tenant_id,lease_id,lease_until)
             VALUES($1,$2,now()+($3 * interval '1 second'))
             ON CONFLICT (tenant_id) DO UPDATE
             SET lease_id=EXCLUDED.lease_id, lease_until=EXCLUDED.lease_until
             WHERE wallet_auto_top_up_claims.lease_until < now()
             RETURNING tenant_id",
        )
        .bind(tenant_id)
        .bind(lease_id)
        .bind(seconds)
        .fetch_optional(&self.pool)
        .await?;
        Ok(r.is_some())
    }

    async fn release_auto_top_up(&self, tenant_id: Uuid, lease_id: Uuid) -> WalletRepoResult<()> {
        sqlx::query("DELETE FROM wallet_auto_top_up_claims WHERE tenant_id=$1 AND lease_id=$2")
            .bind(tenant_id)
            .bind(lease_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn record_auto_top_up_attempt(&self, attempt: &AutoTopUpAttempt) -> WalletRepoResult<()> {
        sqlx::query(
            "INSERT INTO wallet_auto_top_up_attempts
             (id,tenant_id,amount_units,status,provider_reference,error_message,attempted_at)
             VALUES($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(attempt.id)
        .bind(attempt.tenant_id)
        .bind(attempt.amount_units)
        .bind(&attempt.status)
        .bind(&attempt.provider_reference)
        .bind(&attempt.error_message)
        .bind(attempt.attempted_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn record_balance_notification(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        dedupe_key: &str,
        message: &str,
    ) -> WalletRepoResult<bool> {
        let n=sqlx::query("INSERT INTO wallet_notifications(id,tenant_id,event_type,dedupe_key,message) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant_id,event_type,dedupe_key) DO NOTHING").bind(Uuid::new_v4()).bind(tenant_id).bind(event_type).bind(dedupe_key).bind(message).execute(&self.pool).await?;
        Ok(n.rows_affected() == 1)
    }
    async fn list_balance_notifications(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<domain::WalletNotification>> {
        #[derive(sqlx::FromRow)]
        struct WalletNotificationRow {
            id: Uuid,
            tenant_id: Uuid,
            event_type: String,
            dedupe_key: String,
            message: String,
            read_at: Option<DateTime<Utc>>,
            email_sent_at: Option<DateTime<Utc>>,
            email_processing_at: Option<DateTime<Utc>>,
            created_at: DateTime<Utc>,
        }
        let rows=sqlx::query_as::<_,WalletNotificationRow>("SELECT id,tenant_id,event_type,dedupe_key,message,read_at,email_sent_at,email_processing_at,created_at FROM wallet_notifications WHERE tenant_id=$1 ORDER BY created_at DESC LIMIT $2").bind(tenant_id).bind(limit.clamp(1,100) as i64).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| domain::WalletNotification {
                id: r.id,
                tenant_id: r.tenant_id,
                event_type: r.event_type,
                dedupe_key: r.dedupe_key,
                message: r.message,
                read_at: r.read_at,
                email_sent_at: r.email_sent_at,
                email_processing_at: r.email_processing_at,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn claim_pending_email_notifications(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<domain::WalletNotification>> {
        let mut tx = self.pool.begin().await?;
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow5 {
            id: Uuid,
            tenant_id: Uuid,
            event_type: String,
            dedupe_key: String,
            message: String,
            read_at: Option<DateTime<Utc>>,
            email_sent_at: Option<DateTime<Utc>>,
            email_processing_at: Option<DateTime<Utc>>,
            created_at: DateTime<Utc>,
        }
        let rows=sqlx::query_as::<_,WalletRepoRow5>(
            "WITH picked AS (
                SELECT id FROM wallet_notifications
                WHERE email_sent_at IS NULL
                  AND (email_processing_at IS NULL OR email_processing_at < now() - interval '10 minutes')
                  AND event_type IN ('WARNING','CRITICAL','BLOCKED','TOP_UP_SUCCESS','REFUND_SUCCESS')
                ORDER BY created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT $1
             )
             UPDATE wallet_notifications n
             SET email_processing_at=now()
             FROM picked
             WHERE n.id=picked.id
             RETURNING n.id,n.tenant_id,n.event_type,n.dedupe_key,n.message,n.read_at,n.email_sent_at,n.email_processing_at,n.created_at"
        ).bind(limit.clamp(1,100) as i64).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|r| domain::WalletNotification {
                id: r.id,
                tenant_id: r.tenant_id,
                event_type: r.event_type,
                dedupe_key: r.dedupe_key,
                message: r.message,
                read_at: r.read_at,
                email_sent_at: r.email_sent_at,
                email_processing_at: r.email_processing_at,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn mark_balance_notification_emailed(&self, id: Uuid) -> WalletRepoResult<bool> {
        let r=sqlx::query("UPDATE wallet_notifications SET email_sent_at=now(),email_processing_at=NULL WHERE id=$1 AND email_sent_at IS NULL")
            .bind(id).execute(&self.pool).await?;
        Ok(r.rows_affected() == 1)
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/worker/src/durable_scheduler.rs")"
cat <<'EOF' > "$PROJECT/crates/worker/src/durable_scheduler.rs"
use std::{error::Error, sync::Arc, time::Duration};

use application::ports::SchedulingRepository;
use application::{MeterUsageInput, MeterUsageUseCase, MeteredAttempt};
use async_trait::async_trait;
use chrono::Utc;
use domain::Monitor;
use tokio::task::JoinSet;
use tracing::{error, info, warn};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct CheckExecution {
    pub attempted: bool,
    pub error: Option<String>,
}

impl CheckExecution {
    pub fn success(attempted: bool) -> Self {
        Self {
            attempted,
            error: None,
        }
    }
    pub fn failed(attempted: bool, error: impl Into<String>) -> Self {
        Self {
            attempted,
            error: Some(error.into()),
        }
    }
}

/// Executes the actual network check for a claimed monitor. B17 ships a
/// stub implementation; B18/B19 will replace it with the real HTTP/TCP/
/// DNS/heartbeat check logic and persistence.
#[async_trait]
pub trait CheckExecutor: Send + Sync + Clone + 'static {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution>;
}

/// Blanket implementation so `Arc<T>` can be passed to the scheduler
/// without a special wrapper type.
#[async_trait]
impl<T> CheckExecutor for Arc<T>
where
    T: CheckExecutor + Send + Sync + 'static,
{
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        (**self).execute(monitor).await
    }
}

/// B17 placeholder executor. Logs and succeeds without doing any check
/// work, satisfying the durable scheduler contract without stepping on
/// B18/B19's scope.
#[derive(Clone)]
pub struct StubCheckExecutor;

#[async_trait]
impl CheckExecutor for StubCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        info!(
            monitor_id = %monitor.id.0,
            "B17 placeholder check executor: not performing real check yet"
        );
        Ok(CheckExecution::success(false))
    }
}

pub struct DurableScheduler<R, E> {
    scheduling_repository: R,
    executor: E,
    lease_duration: Duration,
    max_concurrency: usize,
    lease_owner: Uuid,
    metering: Option<Arc<MeterUsageUseCase>>,
}

impl<R, E> DurableScheduler<R, E>
where
    R: SchedulingRepository + Clone + Send + Sync + 'static,
    E: CheckExecutor,
{
    pub fn new(
        scheduling_repository: R,
        executor: E,
        lease_duration: Duration,
        max_concurrency: usize,
    ) -> Self {
        Self {
            scheduling_repository,
            executor,
            lease_duration,
            max_concurrency,
            lease_owner: Uuid::new_v4(),
            metering: None,
        }
    }

    /// Enables PAYG billing for durable monitor executions. The scheduler
    /// supplies the stable execution identity; metering finalizes the charge
    /// after the actual attempt, including failed target/network attempts.
    pub fn with_metering(mut self, metering: Arc<MeterUsageUseCase>) -> Self {
        self.metering = Some(metering);
        self
    }

    pub async fn run(self) -> Result<()> {
        info!("Durable scheduler started");
        let mut tasks: JoinSet<Result<()>> = JoinSet::new();

        loop {
            self.scheduling_repository
                .recover_expired_leases(Utc::now())
                .await?;

            let free_capacity = self.max_concurrency.saturating_sub(tasks.len());
            if free_capacity > 0 {
                let one_time = self.scheduling_repository
                    .claim_one_time_checks(free_capacity as u32, self.lease_owner, self.lease_duration, Utc::now())
                    .await?;
                for claim in one_time {
                    let scheduling_repository = self.scheduling_repository.clone();
                    let executor = self.executor.clone();
                    let metering = self.metering.clone();
                    let lease_owner = self.lease_owner;
                    tasks.spawn(async move {
                        let monitor = claim.monitor;
                        let operation = domain::monitor_operation(monitor.kind, true).as_str().to_owned();
                        let billing_request_id = format!("one-time:{}:{}", claim.request.id, claim.request.execution_id);
                        let result = if let Some(metering) = metering {
                            match metering.execute_classified_attempt(
                                MeterUsageInput { tenant_id: monitor.tenant_id.0, operation, units: 1, request_id: billing_request_id },
                                || {
                                    let executor = executor.clone();
                                    let monitor = monitor.clone();
                                    async move { executor.execute(&monitor).await.map(|o| MeteredAttempt { attempted: o.attempted, error: o.error }) }
                                },
                            ).await {
                                Ok((_, attempt)) => if attempt.attempted { attempt.error } else { Some("operation was not attempted".to_owned()) },
                                Err(application::ChargeForUsageError::OperationNotAttempted(reason)) => Some(reason),
                                Err(err) => Some(err.to_string()),
                            }
                        } else {
                            executor.execute(&monitor).await.err().map(|err| err.to_string())
                        };
                        let status = if result.is_some() { domain::OneTimeCheckRequest::FAILED } else { domain::OneTimeCheckRequest::SUCCEEDED };
                        if let Err(err) = scheduling_repository.complete_one_time_check(claim.request.id, lease_owner, status, result.as_deref()).await {
                            error!(request_id=%claim.request.id, error=%err, "failed to finalize one-time check");
                        }
                        Ok::<(), Box<dyn Error + Send + Sync>>(())
                    });
                }
                let free_capacity = self.max_concurrency.saturating_sub(tasks.len());
                if free_capacity == 0 { continue; }
                let claimed = self
                    .scheduling_repository
                    .claim_due_monitors(
                        free_capacity as u32,
                        self.lease_owner,
                        self.lease_duration,
                        Utc::now(),
                    )
                    .await?;

                info!(count = claimed.len(), "claimed due monitors");

                for claim in claimed {
                    let scheduling_repository = self.scheduling_repository.clone();
                    let executor = self.executor.clone();
                    let metering = self.metering.clone();
                    let lease_owner = self.lease_owner;

                    tasks.spawn(async move {
                        let monitor = claim.monitor;
                        let request_id =
                            format!("monitor-check:{}:{}", monitor.id.0, claim.execution_id);
                        let result = if claim.free_entitled {
                            executor.execute(&monitor).await.map(|_| ()).map_err(|err| err)
                        } else if let Some(metering) = metering {
                            match metering.execute_classified_attempt(
                                MeterUsageInput {
                                    tenant_id: monitor.tenant_id.0,
                                    operation: domain::monitor_operation(monitor.kind, false).as_str().to_owned(),
                                    units: 1,
                                    request_id,
                                },
                                || {
                                    let executor = executor.clone();
                                    let monitor = monitor.clone();
                                    async move {
                                        executor.execute(&monitor).await.map(|outcome| {
                                            MeteredAttempt {
                                                attempted: outcome.attempted,
                                                error: outcome.error,
                                            }
                                        })
                                    }
                                },
                            ).await {
                                Ok(_) => Ok(()),
                                Err(application::ChargeForUsageError::OperationNotAttempted(reason)) => {
                                    warn!(monitor_id=%monitor.id.0, reason=%reason, "monitor execution did not reach a billable attempt");
                                    Ok(())
                                }
                                Err(err) => Err(Box::new(err) as Box<dyn Error + Send + Sync>),
                            }
                        } else {
                            executor.execute(&monitor).await.map(|_| ()).map_err(|err| err)
                        };
                        if let Err(err) = &result {
                            warn!(
                                monitor_id = %monitor.id.0,
                                error = %err,
                                "check execution failed"
                            );
                        }

                        // Always release the claim after an execution attempt.
                        // If the process crashes before this point, the lease
                        // will expire and `recover_expired_leases` will make
                        // the monitor claimable again.
                        if let Err(err) = scheduling_repository
                            .complete_claim(monitor.id, lease_owner)
                            .await
                        {
                            error!(
                                monitor_id = %monitor.id.0,
                                error = %err,
                                "failed to release claim"
                            );
                        }

                        result
                    });
                }
            }

            if tasks.is_empty() {
                // No due monitors and no running tasks; avoid a hot loop.
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else {
                // Wait for at least one task to finish before the next
                // claim/recovery iteration. This bounds concurrency.
                match tasks.join_next().await {
                    Some(Ok(Ok(_))) => {}
                    Some(Ok(Err(err))) => {
                        warn!(error = %err, "monitor check task returned an error");
                    }
                    Some(Err(join_err)) => {
                        error!(error = %join_err, "monitor check task panicked");
                    }
                    None => {}
                }
            }
        }
    }
}

/// Convenience entry point used by the worker binary.
pub async fn run_durable<R, E>(
    scheduling_repository: R,
    executor: E,
    lease_duration: Duration,
    max_concurrency: usize,
) -> Result<()>
where
    R: SchedulingRepository + Clone + Send + Sync + 'static,
    E: CheckExecutor,
{
    DurableScheduler::new(
        scheduling_repository,
        executor,
        lease_duration,
        max_concurrency,
    )
    .run()
    .await
}
EOF

mkdir -p "$(dirname "$PROJECT/crates/worker/src/proxy_checker.rs")"
cat <<'EOF' > "$PROJECT/crates/worker/src/proxy_checker.rs"
//! Worker-side execution of a `ProxyCheckJob`: checks every `ProxyEntry`
//! belonging to the job concurrently (bounded by a semaphore), persists
//! each entry's result as it finishes, then marks the job completed.
//!
//! SSRF defense: every entry is resolved/validated via
//! `crate::ssrf::resolve_and_validate_destination` (Layer 2 — DNS
//! resolution + private-IP rejection + address pinning) immediately
//! before connecting, exactly as `crate::checker::check_monitor` does for
//! monitor URLs. No new SSRF logic is implemented here; this module only
//! calls the existing shared validator.
//!
//! Protocol verification is manual and pinned to the validated
//! `SocketAddr`:
//! - HTTP proxies are verified with an absolute-form HTTP GET to
//!   `http://example.com/`.
//! - HTTPS proxies are first connected over TLS to the proxy, then
//!   verified with a CONNECT tunnel to `example.com:443` and a TLS
//!   handshake through that tunnel.
//! - SOCKS5 proxies are verified with a full SOCKS5 handshake to
//!   `example.com:443`, then a TLS handshake through the tunnel.
//!
//! `supports_https` is set only when a TLS handshake through the proxy to
//! `example.com:443` actually succeeds.
//!
//! Response size limits and timeouts bound all proxy-generated data.

use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

use application::ports::ProxyCheckJobRepository;
use application::{MeterUsageInput, MeterUsageUseCase, MeteredAttempt};
use base64::Engine;
use domain::ProxyScheme;
use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::value_objects::{ProxyCredentials, ProxyResult};
use rustls::pki_types::ServerName;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::Semaphore,
    task::JoinSet,
    time::timeout,
};
use tracing::{error, info, warn};

/// Upper bound on how many proxy entries within a single job are checked
/// concurrently. Keeps one bulk job from monopolizing this worker
/// process's connection/file-descriptor budget while other monitors,
/// flows, and jobs belonging to other users are also being processed
/// concurrently in the same process.
const MAX_CONCURRENT_CHECKS_PER_JOB: usize = 20;

/// Per-entry timeout for the TCP connect attempt.
const PER_ENTRY_TIMEOUT_SECS: u64 = 10;

/// Per-protocol-step timeout (read/write/TLS handshake) within a proxy
/// check. A malicious/slow proxy must not stall a job indefinitely.
const PROTOCOL_TIMEOUT_SECS: u64 = 5;

/// Maximum number of bytes accepted from a proxy when reading HTTP
/// response/status headers. Anything larger is treated as a protocol
/// failure.
const MAX_PROXY_RESPONSE_HEADER_BYTES: usize = 8192;

/// Fixed public target used to verify HTTPS/CONNECT/SOCKS tunneling.
/// Deliberately constant, never user input.
const HTTPS_VERIFY_TARGET_HOST: &str = "example.com";
const HTTPS_VERIFY_TARGET_PORT: u16 = 443;

fn proxy_scheme_str(scheme: &ProxyScheme) -> &'static str {
    match scheme {
        ProxyScheme::Http => "http",
        ProxyScheme::Https => "https",
        ProxyScheme::Socks5 => "socks5",
    }
}

fn elapsed_ms(started: Instant) -> u32 {
    started.elapsed().as_millis().try_into().unwrap_or(u32::MAX)
}

fn error_code(scheme: &ProxyScheme, message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("ssrf blocked") {
        return "PRIVATE_IP_BLOCKED";
    }
    if lower.contains("failed to resolve") || lower.contains("did not resolve") {
        return "DNS_FAILED";
    }
    if lower.contains("invalid monitor url")
        || lower.contains("has no host")
        || lower.contains("invalid port")
    {
        return "INVALID_PROXY";
    }
    if lower.contains("connection refused") {
        return "CONNECTION_REFUSED";
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return if lower.contains("target")
            || lower.contains("tls handshake") && lower.contains("target")
        {
            "TARGET_TIMEOUT"
        } else {
            "CONNECTION_TIMEOUT"
        };
    }
    if lower.contains("authentication failed") || lower.contains("auth") && lower.contains("failed")
    {
        return "AUTH_FAILED";
    }
    if lower.contains("tls") {
        return "TLS_FAILED";
    }
    match scheme {
        ProxyScheme::Http => "HTTP_PROXY_FAILED",
        ProxyScheme::Https => "HTTPS_PROXY_FAILED",
        ProxyScheme::Socks5 => "SOCKS5_FAILED",
    }
}

fn proxy_result(
    entry: &ProxyEntry,
    started: Instant,
    reachable: bool,
    supports_https: bool,
    error: Option<String>,
) -> ProxyResult {
    let mut safe_error = error;
    if let Some(message) = safe_error.as_mut() {
        message.truncate(512);
    }
    let error_code = safe_error
        .as_deref()
        .map(|e| error_code(entry.scheme(), e).to_string());
    ProxyResult {
        host: entry.host().as_str().to_string(),
        port: entry.port(),
        scheme: proxy_scheme_str(entry.scheme()).to_string(),
        reachable,
        latency_ms: Some(elapsed_ms(started)),
        supports_https,
        anonymity: None,
        country_code: None,
        error_code,
        error_message: safe_error,
    }
}

async fn write_all_timeout<S>(
    stream: &mut S,
    data: &[u8],
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncWrite + Unpin,
{
    timeout(timeout_duration, stream.write_all(data))
        .await
        .map_err(|_| "timed out writing to proxy".to_string())?
        .map_err(|e| format!("failed to write to proxy: {e}"))?;

    Ok(())
}

async fn read_exact_timeout<S>(
    stream: &mut S,
    buf: &mut [u8],
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + Unpin,
{
    timeout(timeout_duration, stream.read_exact(buf))
        .await
        .map_err(|_| "timed out reading from proxy".to_string())?
        .map_err(|e| format!("failed to read from proxy: {e}"))?;

    Ok(())
}

async fn read_http_header<S>(stream: &mut S, timeout_duration: Duration) -> Result<String, String>
where
    S: AsyncRead + Unpin,
{
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    loop {
        let n = timeout(timeout_duration, stream.read(&mut chunk))
            .await
            .map_err(|_| "timed out waiting for proxy response".to_string())?
            .map_err(|e| format!("failed to read proxy response: {e}"))?;

        if n == 0 {
            return Err("proxy closed connection before completing response headers".to_string());
        }

        if buf.len().saturating_add(n) > MAX_PROXY_RESPONSE_HEADER_BYTES {
            return Err("proxy response headers exceeded size limit".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);

        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    String::from_utf8(buf).map_err(|_| "proxy response was not valid UTF-8".to_string())
}

fn parse_status_code(header: &str) -> Result<u16, String> {
    let first_line = header.lines().next().ok_or("empty proxy response")?;
    let mut parts = first_line.split_whitespace();

    let version = parts.next().ok_or("missing HTTP version")?;
    if !version.starts_with("HTTP/") {
        return Err(format!("invalid HTTP version '{version}'"));
    }

    let status = parts
        .next()
        .ok_or("missing HTTP status code")?
        .parse::<u16>()
        .map_err(|_| "invalid HTTP status code".to_string())?;

    Ok(status)
}

fn proxy_authorization_header(creds: Option<&ProxyCredentials>) -> String {
    match creds {
        Some(creds) => {
            let token = format!("{}:{}", creds.username(), creds.password());
            format!(
                "Proxy-Authorization: Basic {}\r\n",
                base64::engine::general_purpose::STANDARD.encode(token.as_bytes())
            )
        }
        None => String::new(),
    }
}

async fn verify_http_proxy_absolute_form<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let auth = proxy_authorization_header(creds);
    let request = format!(
        "GET http://{HTTPS_VERIFY_TARGET_HOST}/ HTTP/1.1\r\nHost: {HTTPS_VERIFY_TARGET_HOST}\r\nUser-Agent: PingVertex/1.0\r\nConnection: close\r\n{auth}\r\n\r\n"
    );

    write_all_timeout(stream, request.as_bytes(), timeout_duration).await?;
    let header = read_http_header(stream, timeout_duration).await?;
    let status = parse_status_code(&header)?;

    if status == 401 || status == 407 {
        return Err(format!(
            "proxy authentication failed with HTTP status {status}"
        ));
    }
    if !(200..400).contains(&status) {
        return Err(format!("proxy returned HTTP status {status}"));
    }

    Ok(())
}

async fn verify_http_connect_tunnel<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let target = format!("{HTTPS_VERIFY_TARGET_HOST}:{HTTPS_VERIFY_TARGET_PORT}");
    let auth = proxy_authorization_header(creds);
    let request = format!(
        "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nUser-Agent: PingVertex/1.0\r\nProxy-Connection: keep-alive\r\n{auth}\r\n\r\n"
    );

    write_all_timeout(stream, request.as_bytes(), timeout_duration).await?;
    let header = read_http_header(stream, timeout_duration).await?;
    let status = parse_status_code(&header)?;

    if status == 401 || status == 407 {
        return Err(format!(
            "proxy authentication failed with CONNECT status {status}"
        ));
    }
    if !(200..300).contains(&status) {
        return Err(format!("proxy returned CONNECT status {status}"));
    }

    Ok(())
}

async fn tls_handshake<S>(
    stream: S,
    hostname: &str,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let connector = crate::proxy_tls::tls_connector().map_err(|e| e.to_string())?;
    let server_name = ServerName::try_from(hostname.to_string())
        .map_err(|_| "invalid TLS SNI hostname".to_string())?;

    timeout(timeout_duration, connector.connect(server_name, stream))
        .await
        .map_err(|_| "timed out during TLS handshake".to_string())?
        .map_err(|e| format!("TLS handshake failed: {e}"))?;

    Ok(())
}

async fn verify_socks5_proxy<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    target_host: &str,
    target_port: u16,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let methods: &[u8] = if creds.is_some() {
        &[5, 1, 2]
    } else {
        &[5, 1, 0]
    };
    write_all_timeout(stream, methods, timeout_duration).await?;

    let mut method_resp = [0u8; 2];
    read_exact_timeout(stream, &mut method_resp, timeout_duration).await?;

    if method_resp[0] != 5 {
        return Err("SOCKS5 server returned invalid version".to_string());
    }
    if method_resp[1] == 0xFF {
        return Err("SOCKS5 server does not accept any authentication method".to_string());
    }

    match method_resp[1] {
        2 => {
            let creds = creds.ok_or_else(|| {
                "SOCKS5 server selected username/password auth but no credentials were provided"
                    .to_string()
            })?;
            let username = creds.username().as_bytes();
            let password = creds.password().as_bytes();

            if username.len() > 255 || password.len() > 255 {
                return Err("SOCKS5 credentials too long".to_string());
            }

            let mut auth_req = Vec::with_capacity(3 + username.len() + password.len());
            auth_req.push(1);
            auth_req.push(username.len() as u8);
            auth_req.extend_from_slice(username);
            auth_req.push(password.len() as u8);
            auth_req.extend_from_slice(password);

            write_all_timeout(stream, &auth_req, timeout_duration).await?;

            let mut auth_resp = [0u8; 2];
            read_exact_timeout(stream, &mut auth_resp, timeout_duration).await?;

            if auth_resp[0] != 1 || auth_resp[1] != 0 {
                return Err("SOCKS5 username/password authentication failed".to_string());
            }
        }
        0 => {}
        other => {
            return Err(format!(
                "SOCKS5 server selected unsupported authentication method {other}"
            ));
        }
    }

    let host_bytes = target_host.as_bytes();
    if host_bytes.len() > 255 {
        return Err("target host too long for SOCKS5".to_string());
    }

    let mut connect_req = Vec::with_capacity(7 + host_bytes.len());
    connect_req.push(5);
    connect_req.push(1);
    connect_req.push(0);
    connect_req.push(3);
    connect_req.push(host_bytes.len() as u8);
    connect_req.extend_from_slice(host_bytes);
    connect_req.extend_from_slice(&target_port.to_be_bytes());

    write_all_timeout(stream, &connect_req, timeout_duration).await?;

    let mut connect_head = [0u8; 4];
    read_exact_timeout(stream, &mut connect_head, timeout_duration).await?;

    if connect_head[0] != 5 {
        return Err("SOCKS5 connect reply had invalid version".to_string());
    }
    if connect_head[1] != 0 {
        return Err(format!(
            "SOCKS5 connect failed with reply code {}",
            connect_head[1]
        ));
    }

    match connect_head[3] {
        1 => {
            let mut rest = [0u8; 6];
            read_exact_timeout(stream, &mut rest, timeout_duration).await?;
        }
        4 => {
            let mut rest = [0u8; 18];
            read_exact_timeout(stream, &mut rest, timeout_duration).await?;
        }
        3 => {
            let mut len_buf = [0u8; 1];
            read_exact_timeout(stream, &mut len_buf, timeout_duration).await?;
            let len = len_buf[0] as usize;
            let mut addr = vec![0u8; len];
            read_exact_timeout(stream, &mut addr, timeout_duration).await?;
            let mut port_buf = [0u8; 2];
            read_exact_timeout(stream, &mut port_buf, timeout_duration).await?;
        }
        _ => return Err("SOCKS5 connect reply had invalid address type".to_string()),
    }

    Ok(())
}

/// Checks a single proxy entry and returns its `ProxyResult`.
///
/// Steps:
/// 1. Build a `scheme://host:port` URL from the entry's own fields and
///    run it through `resolve_and_validate_destination` — this performs
///    the DNS lookup exactly once, rejects any private/reserved resolved
///    address (Layer 2 SSRF defense, on top of `ProxyHost::new`'s
///    literal-IP check already performed at submission time), and
///    returns the pinned `SocketAddr` to connect to.
/// 2. Connect to that pinned address and perform the protocol-specific
///    verification described in the module docs.
///
/// A resolution/validation failure and a connection failure are both
/// reported as `reachable: false` with a descriptive `error`, never as a
/// panic or a propagated `Result` — one bad proxy entry must never stop
/// the rest of the job from being checked.
async fn check_single_entry(entry: &ProxyEntry) -> (ProxyResult, bool) {
    let target = format!(
        "{}://{}:{}",
        proxy_scheme_str(entry.scheme()),
        entry.host().as_str(),
        entry.port()
    );

    let started = Instant::now();

    let destination = match crate::ssrf::resolve_and_validate_public_destination(&target).await {
        Ok(destination) => destination,
        Err(err) => {
            let result = proxy_result(entry, started, false, false, Some(err.clone()));
            let code = result.error_code.as_deref().unwrap_or_default();
            let attempted = code != "PRIVATE_IP_BLOCKED" && code != "INVALID_PROXY";
            return (result, attempted);
        }
    };

    let connect_timeout = Duration::from_secs(PER_ENTRY_TIMEOUT_SECS);
    let protocol_timeout = Duration::from_secs(PROTOCOL_TIMEOUT_SECS);

    let (reachable, supports_https, error) = match entry.scheme() {
        ProxyScheme::Http => {
            match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
                Ok(Ok(mut stream)) => {
                    match verify_http_proxy_absolute_form(
                        &mut stream,
                        entry.credentials(),
                        protocol_timeout,
                    )
                    .await
                    {
                        Err(err) => (false, false, Some(err)),
                        Ok(()) => {
                            match timeout(
                                connect_timeout,
                                TcpStream::connect(destination.socket_addr),
                            )
                            .await
                            {
                                Ok(Ok(mut tunnel_stream)) => {
                                    match verify_http_connect_tunnel(
                                        &mut tunnel_stream,
                                        entry.credentials(),
                                        protocol_timeout,
                                    )
                                    .await
                                    {
                                        Ok(()) => match tls_handshake(
                                            tunnel_stream,
                                            HTTPS_VERIFY_TARGET_HOST,
                                            protocol_timeout,
                                        )
                                        .await
                                        {
                                            Ok(()) => (true, true, None),
                                            Err(err) => (
                                                true,
                                                false,
                                                Some(format!("HTTPS target TLS failed: {err}")),
                                            ),
                                        },
                                        Err(err) => (
                                            true,
                                            false,
                                            Some(format!("HTTPS tunnel failed: {err}")),
                                        ),
                                    }
                                }
                                Ok(Err(err)) => (
                                    true,
                                    false,
                                    Some(format!("HTTPS tunnel connection failed: {err}")),
                                ),
                                Err(_) => (
                                    true,
                                    false,
                                    Some("HTTPS tunnel connection timed out".to_string()),
                                ),
                            }
                        }
                    }
                }
                Ok(Err(err)) => (false, false, Some(err.to_string())),
                Err(_) => (
                    false,
                    false,
                    Some(format!(
                        "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
                    )),
                ),
            }
        }
        ProxyScheme::Https => {
            match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
                Ok(Ok(tcp)) => {
                    let connector = match crate::proxy_tls::tls_connector() {
                        Ok(c) => c,
                        Err(err) => {
                            return (
                                proxy_result(entry, started, false, false, Some(err.to_string())),
                                true,
                            );
                        }
                    };
                    let server_name = match ServerName::try_from(entry.host().as_str().to_string())
                    {
                        Ok(name) => name,
                        Err(err) => {
                            return (
                                proxy_result(entry, started, false, false, Some(err.to_string())),
                                true,
                            );
                        }
                    };

                    match timeout(protocol_timeout, connector.connect(server_name, tcp)).await {
                        Ok(Ok(mut proxy_tls)) => {
                            if let Err(err) = verify_http_connect_tunnel(
                                &mut proxy_tls,
                                entry.credentials(),
                                protocol_timeout,
                            )
                            .await
                            {
                                (false, false, Some(err))
                            } else if let Err(err) =
                                tls_handshake(proxy_tls, HTTPS_VERIFY_TARGET_HOST, protocol_timeout)
                                    .await
                            {
                                (false, false, Some(err))
                            } else {
                                (true, true, None)
                            }
                        }
                        Ok(Err(err)) => (false, false, Some(format!("TLS to proxy failed: {err}"))),
                        Err(_) => (
                            false,
                            false,
                            Some("TLS handshake with proxy timed out".to_string()),
                        ),
                    }
                }
                Ok(Err(err)) => (false, false, Some(err.to_string())),
                Err(_) => (
                    false,
                    false,
                    Some(format!(
                        "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
                    )),
                ),
            }
        }
        ProxyScheme::Socks5 => {
            match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
                Ok(Ok(mut stream)) => {
                    if let Err(err) = verify_socks5_proxy(
                        &mut stream,
                        entry.credentials(),
                        HTTPS_VERIFY_TARGET_HOST,
                        HTTPS_VERIFY_TARGET_PORT,
                        protocol_timeout,
                    )
                    .await
                    {
                        (false, false, Some(err))
                    } else if let Err(err) =
                        tls_handshake(stream, HTTPS_VERIFY_TARGET_HOST, protocol_timeout).await
                    {
                        (
                            true,
                            false,
                            Some(format!(
                                "SOCKS5 tunnel connected but TLS to target failed: {err}"
                            )),
                        )
                    } else {
                        (true, true, None)
                    }
                }
                Ok(Err(err)) => (false, false, Some(err.to_string())),
                Err(_) => (
                    false,
                    false,
                    Some(format!(
                        "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
                    )),
                ),
            }
        }
    };

    let result = proxy_result(entry, started, reachable, supports_https, error);
    (result, true)
}

/// Runs every entry of `job` through `check_single_entry`, persisting
/// each result and the job's overall progress/status as it goes.
///
/// `job` must already have been persisted (via
/// `ProxyCheckJobRepository::save_job`, called by the application layer's
/// submit use case) and `entries` must be exactly the entries belonging
/// to it. This function's only responsibilities are: transition the job
/// to `Running` (an optimistic-lock no-op if some other worker already
/// picked it up — see `ProxyCheckJob::mark_running`'s doc comment), check
/// every entry concurrently (bounded by `MAX_CONCURRENT_CHECKS_PER_JOB`),
/// persist each entry's result via `update_entry`, and finally mark the
/// job `Completed` and persist that.
///
/// Dispatch (how a submitted job reaches this function in the first
/// place — polling, LISTEN/NOTIFY, a channel, etc.) is out of scope here
/// and left to a separate prompt, per the task boundary for this module.
pub async fn run_proxy_check_job<R>(
    repository: &R,
    mut job: ProxyCheckJob,
    entries: Vec<ProxyEntry>,
    worker_id: &str,
    metering: Option<Arc<MeterUsageUseCase>>,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: ProxyCheckJobRepository + ?Sized,
{
    // The worker already performed the atomic pending -> running claim.
    // The executor MUST NOT own or repeat that state transition.
    if job.status() != domain::ProxyCheckJobStatus::Running {
        info!(job_id = %job.id(), "proxy check job is not running; skipping");
        return Ok(());
    }

    if entries.len() != job.total_count() as usize {
        let mut failed = job.clone();
        if failed.mark_failed(format!(
            "proxy entry count mismatch: expected {}, loaded {}",
            job.total_count(),
            entries.len()
        )) {
            repository.update_job(&failed).await?;
        }
        return Ok(());
    }

    let persisted_successes = entries
        .iter()
        .filter(|e| e.result().is_some_and(|r| r.reachable))
        .count() as u32;
    let persisted_failures = entries
        .iter()
        .filter(|e| e.result().is_some_and(|r| !r.reachable))
        .count() as u32;
    job.rebuild_progress(persisted_successes, persisted_failures);

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS_PER_JOB));
    let mut tasks = JoinSet::new();
    for entry in entries {
        let had_existing_result = entry.result().is_some();
        let semaphore = Arc::clone(&semaphore);
        let metering = metering.clone();
        let job_for_task = job.clone();

        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("semaphore remains open");
            let mut entry = entry;
            let entry_id = entry.id();
            let job_id = job_for_task.id();
            let (result, billing_blocked) = if let Some(metering) = metering.clone() {
                let request_id = format!("proxy-check:{}:{}", job_id, entry_id);
                if had_existing_result && metering.find_by_request(job_for_task.tenant_id().0, &request_id).await?.is_some() {
                    (entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("PAYG execution was already finalized".to_owned()))), false)
                } else {
                match metering.execute_classified_attempt_with_output(
                    MeterUsageInput {
                        tenant_id: job_for_task.tenant_id().0,
                        operation: domain::PaygOperation::ProxyOneTime.as_str().to_owned(),
                        units: 1,
                        request_id,
                    },
                    || async {
                        if had_existing_result {
                            let result = entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("persisted proxy result missing".to_owned())));
                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted: true, error: None }, result))
                        } else {
                            let (result, attempted) = check_single_entry(&entry).await;
                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted, error: None }, result))
                        }
                    },
                ).await {
                    Ok((_, Some(result))) => (result, false),
                    Ok((_, None)) => {
                        (entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("PAYG execution was already finalized".to_owned()))), false)
                    }
                    Err(application::ChargeForUsageError::InsufficientBalance) => {
                        let result = entry.result().cloned().unwrap_or_else(|| proxy_result(
                            &entry, Instant::now(), false, false,
                            Some("PAYG balance exhausted; top up the wallet to resume proxy checks".to_owned()),
                        ));
                        (result, true)
                    }
                    Err(application::ChargeForUsageError::OperationNotAttempted(reason)) => {
                        let result = proxy_result(&entry, Instant::now(), false, false, Some(reason));
                        (result, false)
                    }
                    Err(err) => return Err(Box::new(err) as Box<dyn Error + Send + Sync>),
                }
                }
            } else {
                let (result, _) = check_single_entry(&entry).await;
                (result, false)
            };
            entry.set_result(result);
            Ok((entry, !had_existing_result, billing_blocked))
        });
    }

    let mut fatal_reason: Option<String> = None;
    let mut billing_blocked = false;
    let heartbeat_every = Duration::from_secs(15);

    loop {
        match timeout(heartbeat_every, tasks.join_next()).await {
            Ok(Some(Ok(Ok((entry, should_count, entry_billing_blocked))))) => {
                billing_blocked |= entry_billing_blocked;
                if let Err(err) = repository.update_entry(&entry, job.tenant_id()).await {
                    fatal_reason.get_or_insert_with(|| {
                        format!("failed to persist proxy entry result: {err}")
                    });
                    error!(job_id=%job.id(), entry_id=%entry.id(), error=%err, "proxy result persistence failed");
                    continue;
                }

                if should_count {
                    job.record_result(entry.result().is_some_and(|r| r.reachable));
                }

                // Persist progress periodically rather than once per proxy;
                // the entry result itself is already durably stored. This keeps
                // bulk jobs cheap while the separate heartbeat preserves the lease.
                if (job.checked_count() == job.total_count()
                    || job.checked_count().is_multiple_of(10))
                    && let Err(err) = repository.update_job(&job).await
                {
                    fatal_reason.get_or_insert_with(|| {
                        format!("failed to persist proxy check job progress: {err}")
                    });
                    error!(
                        job_id=%job.id(),
                        error=%err,
                        "proxy job progress persistence failed"
                    );
                }

                // Lease ownership is separate from job progress so a failed
                // progress write cannot accidentally hide the heartbeat.
                match repository.heartbeat(job.id(), worker_id).await {
                    Ok(true) => {}
                    Ok(false) => {
                        fatal_reason.get_or_insert_with(|| "proxy job lease was lost".to_string());
                        break;
                    }
                    Err(err) => {
                        fatal_reason.get_or_insert_with(|| {
                            format!("failed to heartbeat proxy check job: {err}")
                        });
                        error!(job_id=%job.id(), error=%err, "proxy job heartbeat failed");
                        break;
                    }
                }
            }
            Ok(Some(Ok(Err(err)))) => {
                fatal_reason.get_or_insert_with(|| format!("proxy entry task failed: {err}"));
                warn!(job_id=%job.id(), error=%err, "proxy entry task failed");
            }
            Ok(Some(Err(join_err))) => {
                fatal_reason
                    .get_or_insert_with(|| format!("proxy entry task join failed: {join_err}"));
                warn!(job_id=%job.id(), error=%join_err, "proxy entry task join failed");
            }
            Ok(None) => break,
            Err(_) => match repository.heartbeat(job.id(), worker_id).await {
                Ok(true) => {}
                Ok(false) => {
                    fatal_reason.get_or_insert_with(|| "proxy job lease was lost".to_string());
                    break;
                }
                Err(err) => {
                    fatal_reason.get_or_insert_with(|| {
                        format!("failed to heartbeat proxy check job: {err}")
                    });
                    break;
                }
            },
        }
    }

    if billing_blocked {
        // Keep the durable job Running when one or more entries were blocked by
        // insufficient PAYG balance. The lease will expire, stale-job recovery
        // will return it to Pending, and a later sweep can retry after top-up.
        repository.update_job(&job).await?;
        return Ok(());
    }

    if let Some(reason) = fatal_reason {
        if job.status() == domain::ProxyCheckJobStatus::Running && job.mark_failed(reason) {
            repository.update_job(&job).await?;
        }
        return Ok(());
    }

    if job.checked_count() != job.total_count()
        || job.checked_count() != job.successful_count().saturating_add(job.failed_count())
    {
        if job.mark_failed("proxy result persistence incomplete") {
            repository.update_job(&job).await?;
        }
        return Ok(());
    }

    if job.mark_completed() {
        repository.update_job(&job).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::IdGenerator;
    use domain::shared::ids::{TenantId, UserId};
    use domain::value_objects::ProxyHost;
    use infra::{InMemoryProxyCheckJobRepository, UuidIdGenerator};
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn run_proxy_check_job_marks_job_completed_and_records_entry_result() {
        let repo = InMemoryProxyCheckJobRepository::new();
        let id_generator = UuidIdGenerator::new();

        let user_id = UserId::new();
        let job_id = id_generator.next_proxy_check_job_id().await;
        let job = ProxyCheckJob::new(job_id, user_id, TenantId::new(), 1);

        let entry_id = id_generator.next_proxy_entry_id().await;
        let host = ProxyHost::new("localhost").unwrap();
        let entry = ProxyEntry::new(entry_id, job_id, host, 8080, ProxyScheme::Http, None).unwrap();

        repo.save_job(&job, &[entry]).await.unwrap();

        // The worker owns the atomic Pending -> Running claim.
        assert!(repo.try_mark_running(job_id, "test-worker").await.unwrap());

        let running_job = repo
            .find_job_by_id(job_id, job.tenant_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(running_job.status(), domain::ProxyCheckJobStatus::Running);

        let entries = repo
            .find_entries_by_job_id(job_id, job.tenant_id())
            .await
            .unwrap();

        run_proxy_check_job(&repo, running_job, entries, "test-worker", None)
            .await
            .unwrap();

        let job_after = repo
            .find_job_by_id(job_id, job.tenant_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job_after.status(), domain::ProxyCheckJobStatus::Completed);
        assert_eq!(job_after.checked_count(), 1);

        let entries_after = repo
            .find_entries_by_job_id(job_id, job.tenant_id())
            .await
            .unwrap();
        assert_eq!(entries_after.len(), 1);
        assert!(entries_after[0].result().is_some());
    }

    #[test]
    fn parse_status_code_accepts_200() {
        assert_eq!(parse_status_code("HTTP/1.1 200 OK\r\n\r\n").unwrap(), 200);
    }

    #[test]
    fn parse_status_code_rejects_non_http() {
        assert!(parse_status_code("SOCKS5\r\n").is_err());
    }

    #[tokio::test]
    async fn http_proxy_verification_accepts_2xx() {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await;
            }
        });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let creds = ProxyCredentials::new("user", "pass").ok();

        assert!(
            verify_http_proxy_absolute_form(&mut stream, creds.as_ref(), Duration::from_secs(2))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn http_proxy_verification_rejects_oversized_headers() {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let _ = stream.read(&mut [0u8; 4096]).await;
                let payload = vec![b'a'; MAX_PROXY_RESPONSE_HEADER_BYTES + 1024];
                let _ = stream.write_all(&payload).await;
                let _ = stream.flush().await;
            }
        });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result =
            verify_http_proxy_absolute_form(&mut stream, None, Duration::from_secs(2)).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("size limit"));
    }
}
EOF

mkdir -p "$(dirname "$PROJECT/migrations/0091_payg_blueprint_part1.sql")"
cat <<'EOF' > "$PROJECT/migrations/0091_payg_blueprint_part1.sql"
BEGIN;

-- Preserve historical pricing configs. PAYG migration creates a new effective
-- version instead of mutating an existing pricing snapshot.
INSERT INTO pricing_configs (
    id, version, currency, cost_per_check, effective_storage_bytes_per_check,
    cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold, backup_multiplier,
    markup_multiplier, min_interval_seconds, max_monitors, max_retention_days,
    min_paid_monthly_price, free_monitors, free_interval_seconds,
    free_retention_days, created_at, effective_at, created_by
)
SELECT
    gen_random_uuid(), version + 1, currency, cost_per_check,
    effective_storage_bytes_per_check, cost_per_gb_hot, cost_per_gb_warm,
    cost_per_gb_cold, backup_multiplier, markup_multiplier,
    min_interval_seconds, max_monitors, max_retention_days,
    min_paid_monthly_price, 10, 300, free_retention_days,
    now(), now(), created_by
FROM pricing_configs
ORDER BY version DESC
LIMIT 1;

-- Versioned, server-controlled PAYG prices. Values are micro-USD per unit.
INSERT INTO payg_pricing_configs
    (id, version, operation, unit_name, unit_price, currency,
     effective_from, effective_until, active)
VALUES
    (gen_random_uuid(), 3, 'http_check.one_time',       'check',       2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'tcp_check.one_time',        'check',       3,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'dns_check.one_time',        'check',       3,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'api_flow_check.one_time',   'request/check',4,   'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'proxy_check.one_time',      'check',       2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'http_check.monitoring',     'check',       2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'tcp_check.monitoring',      'check',       3,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'dns_check.monitoring',      'check',       3,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'heartbeat_processing',      'check',       1,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'api_flow_check.monitoring', 'request/check',4,   'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'proxy_check.monitoring',    'check',       2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'rum_event',                 'event',       1,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'sms',                       'message',     1000, 'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'storage',                   'MB/month',    20,   'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'network_egress',            'MB',          100,  'USD', now(), NULL, TRUE)
ON CONFLICT (version, operation) DO NOTHING;

COMMIT;
EOF

mkdir -p "$(dirname "$PROJECT/migrations/0092_payg_one_time_checks.sql")"
cat <<'EOF' > "$PROJECT/migrations/0092_payg_one_time_checks.sql"
BEGIN;
CREATE TABLE IF NOT EXISTS one_time_check_requests (
 id UUID PRIMARY KEY,
 tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
 monitor_id UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
 created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
 request_id TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('QUEUED','RUNNING','SUCCEEDED','FAILED')),
 execution_id UUID NOT NULL,
 lease_owner UUID,
 lease_expires_at TIMESTAMPTZ,
 error TEXT,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 started_at TIMESTAMPTZ,
 completed_at TIMESTAMPTZ,
 CONSTRAINT uq_one_time_check_request UNIQUE(tenant_id,request_id),
 CONSTRAINT uq_one_time_check_execution UNIQUE(execution_id)
);
CREATE INDEX IF NOT EXISTS idx_one_time_check_queue ON one_time_check_requests(status,created_at);
CREATE INDEX IF NOT EXISTS idx_one_time_check_lease ON one_time_check_requests(lease_expires_at) WHERE status='RUNNING';
COMMIT;
EOF

echo
echo "PAYG Blueprint Part 1 source changes applied."
echo "Next: run the project test protocol manually:"
echo "  cargo fmt --all"
echo "  cargo clippy --workspace --all-targets -- -D warnings"
echo "  cargo test --workspace"
echo "  # then run the project E2E/HTTP+auth+PostgreSQL+Redis suite"
echo
echo "If a test fails, do NOT rerun this whole script. Patch only the failing file with a targeted cat <<\x27EOF\x27 block."
