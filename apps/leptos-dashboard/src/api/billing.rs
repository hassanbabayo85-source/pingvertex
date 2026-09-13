#![allow(dead_code)]
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
    /// Present on `POST /billing/addons` responses (a checkout session
    /// was just created). Absent on `GET /billing/summary` rows.
    #[serde(default)]
    pub checkout_session_id: Option<String>,
    /// Provider checkout URL the buyer must visit to pay. Only set on
    /// the create response. Surfaced in the Add-ons page so the buyer
    /// can complete payment.
    #[serde(default)]
    pub checkout_url: Option<String>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Subscription {
    pub id: String,
    pub tenant_id: String,
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

/// PAYG billing summary: wallet balance + active add-ons.
///
/// This replaces the subscription-era "base price + add-ons" shape. In
/// PAYG there is no base subscription price — the wallet is funded by
/// top-ups, and add-ons are separately purchased line items.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BillingSummary {
    pub wallet_balance_units: i64,
    pub wallet_currency: String,
    pub active_add_ons: Vec<AddOn>,
    pub add_ons_total: String,
    pub currency: String,
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
pub struct AddOnRequest {
    pub addon_type: String,
    pub quantity: u64,
    pub country_code: Option<String>,
    pub tier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaygPricing {
    pub operation: String,
    pub unit_name: String,
    pub unit_price: i64,
    pub currency: String,
    pub version: i64,
    pub effective_from: String,
}

/// The tenant's current free-tier entitlement, resolved server-side from
/// the active DB pricing config. The UI must display these values
/// verbatim rather than hardcoding a monitor count or minimum interval.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FreeTier {
    pub max_monitors: u32,
    pub min_check_interval_seconds: u32,
    pub retention_days: u32,
    #[serde(default)]
    pub max_retention_days: u32,
    #[serde(default)]
    pub default_retention_days: u32,
}

/// Server-owned preview limits. The frontend MUST read these from the API
/// rather than hardcoding `30`, `32`, or `3650`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PreviewLimits {
    pub max_regions: u32,
    pub max_forecast_days: u32,
    pub min_interval_seconds_http: u32,
    pub min_interval_seconds_tcp: u32,
    pub min_interval_seconds_dns: u32,
    pub min_interval_seconds_heartbeat: u32,
    pub min_interval_seconds_flow: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaygPricingPage {
    pub prices: Vec<PaygPricing>,
    pub free_tier: FreeTier,
    /// Minimum wallet top-up in micro-USD. Server-owned (see
    /// `domain::MIN_PAYG_TOP_UP_UNITS`); the UI must not hardcode "$5".
    pub payg_min_top_up_units: i64,
    pub preview_limits: PreviewLimits,
}

pub async fn payg_pricing() -> Result<PaygPricingPage, ApiError> {
    client::get_json("/payg/pricing").await
}

#[derive(Debug, Clone, Serialize)]
pub struct PaygPreviewRequest {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub regions: u32,
    pub forecast_days: u32,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PaygPreview {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub regions: u32,
    pub forecast_days: u32,
    pub checks_per_day: u64,
    pub checks_total: u64,
    pub price_per_check_units: i64,
    pub currency: String,
    pub pricing_version: i64,
    pub estimated_cost_units: i64,
    pub estimate_label: String,
}

pub async fn payg_preview(req: &PaygPreviewRequest) -> Result<PaygPreview, ApiError> {
    client::post_json("/payg/preview", req).await
}

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

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Allowance {
    pub cycle_started_at: String,
    pub cycle_ends_at: String,
    pub sms_limit: i32,
    pub sms_used: i32,
    pub sms_remaining: i32,
    pub voice_limit: i32,
    pub voice_used: i32,
    pub phone_numbers_limit: i32,
    pub phone_numbers_used: i32,
    pub enabled_monitors: u64,
    pub retention_days: u32,
    pub max_retention_days: u32,
    pub default_retention_days: u32,
    pub retention_source: String,
}

pub async fn allowance() -> Result<Allowance, String> {
    client::get_json::<Allowance>("/billing/allowance")
        .await
        .map_err(|e| e.to_string())
}
