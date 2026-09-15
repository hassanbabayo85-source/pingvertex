#![allow(dead_code)]
use crate::api::client::{self, ApiError};
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
    #[serde(default)]
    pub checkout_session_id: Option<String>,
    #[serde(default)]
    pub checkout_url: Option<String>,
}

/// PAYG billing summary: wallet balance + active add-ons.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BillingSummary {
    pub wallet_balance_units: i64,
    pub wallet_currency: String,
    pub active_add_ons: Vec<AddOn>,
    pub add_ons_total: String,
    pub currency: String,
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

pub async fn summary() -> Result<BillingSummary, ApiError> {
    client::get_json("/billing/summary").await
}

pub async fn add_on(req: &AddOnRequest) -> Result<AddOn, ApiError> {
    client::post_json("/billing/addons", req).await
}

// 🟡 MATAKI NA 3: An gano gibin Frontend - Ƙara function na soke Add-On (Cancel Add-on)
pub async fn cancel_add_on(id: &str) -> Result<(), ApiError> {
    client::post_json(
        "/billing/addons/cancel",
        &serde_json::json!({ "id": id }),
    ).await
}

// 🟡 MATAKI NA 3: An gano gibin Frontend - Ƙara function na tantance Add-On (Verify Add-on)
pub async fn verify_add_on(id: &str) -> Result<(), ApiError> {
    client::post_json(
        "/billing/addons/verify",
        &serde_json::json!({ "id": id }),
    ).await
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Invoice {
    pub id: String,
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

// 🟡 MATAKI NA 3: An gano gibin Frontend - Ƙara function na ɗauko cikakken Invoice guda ɗaya
pub async fn get_invoice_details(id: &str) -> Result<Invoice, ApiError> {
    client::get_json(&format!("/billing/invoices/{}", id)).await
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
