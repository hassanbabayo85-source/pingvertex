use crate::{error::ApiError, state::AppState};
use application::ports::RefundRequest;
use axum::{
    Json,
    extract::{Extension, Query, State},
};
use chrono::{DateTime, Duration, Utc};
use domain::shared::types::AuthenticatedUser;
use domain::{WalletSettings, WalletTransactionType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct WalletResponse {
    pub wallet_id: String,
    pub currency: String,
    pub balance_units: i64,
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
    let level = status(
        wallet.balance_units,
        settings.low_balance_threshold_units,
        settings.critical_balance_threshold_units,
    );
    if level != "OK" {
        let msg = match level.as_str() {
            "BLOCKED" => "PAYG is blocked because your wallet balance is zero.",
            "CRITICAL" => "Your PAYG wallet balance is critically low.",
            _ => "Your PAYG wallet balance is low.",
        };
        let _ = state
            .wallet_repository
            .record_balance_notification(
                tenant_id,
                &level,
                &format!("{}:{}", level, Utc::now().date_naive()),
                msg,
            )
            .await;
    }
    Ok(Json(WalletResponse {
        wallet_id: wallet.id.to_string(),
        currency: wallet.currency,
        status: level,
        balance_units: wallet.balance_units,
        low_balance_threshold_units: settings.low_balance_threshold_units,
        critical_balance_threshold_units: settings.critical_balance_threshold_units,
    }))
}
pub async fn list_wallet_transactions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Vec<WalletTransactionResponse>>, ApiError> {
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
    if !(1_000_000..=1_000_000_000).contains(&req.amount_units) {
        return Err(ApiError::bad_request(
            "top-up must be between $1 and $1,000",
        ));
    }
    let tenant_id = user.tenant_context().tenant_id();
    let d = state
        .payment_provider
        .create_wallet_top_up(&application::ports::WalletTopUpRequest {
            tenant_id,
            amount_units: req.amount_units,
            currency: "USD".to_owned(),
            payment_method: req.payment_method,
        })
        .await
        .map_err(|e| ApiError::internal_with_source("wallet top-up provider error", e))?;
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
    if s.auto_top_up_enabled && s.payment_method.as_deref().unwrap_or("").is_empty() {
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
    if req.amount_units <= 0 || req.amount_units > 1_000_000_000 {
        return Err(ApiError::bad_request("invalid refund amount"));
    }
    let d = state
        .payment_provider
        .refund_payment(&RefundRequest {
            tenant_id: user.tenant_context().tenant_id(),
            provider_payment_id: req.provider_payment_id,
            amount_units: req.amount_units,
            currency: req.currency.unwrap_or_else(|| "USD".to_owned()),
        })
        .await
        .map_err(|e| ApiError::internal_with_source("refund provider error", e))?;
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
    let days = ((to - from).num_seconds() as f64 / 86400.0).max(1.0);
    let projected = (report.month_spend_units as f64 / days * 30.0) as i64;
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

#[allow(dead_code)]
fn _tx_type(_: WalletTransactionType) {}
