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

pub async fn summary() -> Result<BillingSummary, ApiError> {
    client::get_json("/billing/summary").await
}
pub async fn quote(req: &QuoteRequest) -> Result<Quote, ApiError> {
    client::post_json("/pricing/quote", req).await
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
