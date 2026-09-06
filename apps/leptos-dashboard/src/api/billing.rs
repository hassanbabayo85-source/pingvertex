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
pub struct Subscription { pub id:String, pub tenant_id:String, pub plan_tier:String, pub status:String, pub trial_ends_at:Option<String>, pub grace_period_ends_at:Option<String>, pub current_period_ends_at:Option<String>, pub canceled_at:Option<String>, pub cancel_at_period_end:bool, pub monitors:Option<u64>, pub interval_seconds:Option<u32>, pub retention_days:Option<u32>, pub pricing_version:Option<u32> }
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Plan { pub tier:String, pub max_monitors:u32, pub min_check_interval_seconds:u32, pub history_retention_days:u32, pub max_locations:u32, pub max_team_members:u32, pub rum_enabled:bool, pub advanced_features:bool, pub monthly_price:String, pub currency:String }

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

pub async fn subscription() -> Result<Option<Subscription>, ApiError> { client::get_json("/billing/subscription").await }
pub async fn plans() -> Result<Vec<Plan>, ApiError> { client::get_json("/billing/plans").await }
pub async fn cancel_subscription(at_period_end: bool) -> Result<Subscription, ApiError> {
 client::post_json("/billing/subscription/cancel", &serde_json::json!({"at_period_end":at_period_end})).await
}
pub async fn summary() -> Result<BillingSummary, ApiError> {
    client::get_json("/billing/summary").await
}
pub async fn quote(req: &QuoteRequest) -> Result<Quote, ApiError> {
    client::post_json("/billing/quote", req).await
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

