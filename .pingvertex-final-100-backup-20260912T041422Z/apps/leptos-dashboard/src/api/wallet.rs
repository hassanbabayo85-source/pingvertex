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
    pub daily_spend_limit_units: Option<i64>,
    pub monthly_spend_limit_units: Option<i64>,
    pub daily_usage_alert_threshold_units: Option<i64>,
    pub monthly_usage_alert_threshold_units: Option<i64>,
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
    pub daily_spend_limit_units: Option<i64>,
    pub monthly_spend_limit_units: Option<i64>,
    pub daily_usage_alert_threshold_units: Option<i64>,
    pub monthly_usage_alert_threshold_units: Option<i64>,
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
