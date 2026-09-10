use crate::{error::ApiError, state::AppState};
use application::ports::{PaymentVerification, RefundRequest};
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
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
    pub daily_spend_limit_units: Option<i64>,
    pub monthly_spend_limit_units: Option<i64>,
    pub daily_usage_alert_threshold_units: Option<i64>,
    pub monthly_usage_alert_threshold_units: Option<i64>,
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

fn status(payg_activated: bool, balance: i64, low: i64, critical: i64) -> String {
    if !payg_activated {
        return "FREE".to_owned();
    }
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
    let level = status(
        wallet.payg_activated_at.is_some(),
        wallet.balance_units,
        settings.low_balance_threshold_units,
        settings.critical_balance_threshold_units,
    );
    let balance = state
        .wallet_repository
        .get_balance_summary(tenant_id, "USD")
        .await
        .map_err(|_| ApiError::internal("wallet unavailable"))?;
    if level != "OK" && level != "FREE" {
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
    if req.amount_units < domain::MIN_PAYG_TOP_UP_UNITS {
        return Err(ApiError::bad_request("minimum PAYG top-up is $5"));
    }
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
            customer_email: state
                .platform_user_repository
                .find_by_id(&user.user_id())
                .await
                .map_err(|e| ApiError::internal_with_source("failed to load billing customer", e))?
                .ok_or_else(|| ApiError::internal("billing customer account not found"))?
                .email()
                .to_string(),
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
pub async fn verify_wallet_top_up(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(provider_checkout_id): Path<String>,
) -> Result<Json<TopUpResponse>, ApiError> {
    user.require(Permission::ManageBilling)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    let tenant_id = user.tenant_context().tenant_id().0;
    let session = state
        .wallet_repository
        .find_top_up_session(tenant_id, &provider_checkout_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load wallet top-up session", e))?
        .ok_or_else(|| ApiError::not_found("wallet top-up session not found"))?;
    let provider_payment_id = match session.status.as_str() {
        WalletTopUpSession::COMPLETED => session.provider_payment_id.clone(),
        _ => match state
            .payment_provider
            .verify_payment(&session.provider_checkout_id)
            .await
            .map_err(|e| ApiError::internal_with_source("failed to verify wallet top-up", e))?
        {
            PaymentVerification::Paid {
                provider_payment_id,
                amount_units,
                currency,
                ..
            } => {
                if amount_units != session.amount_units
                    || !currency.eq_ignore_ascii_case(&session.currency)
                {
                    return Err(ApiError::conflict(
                        "provider amount or currency does not match wallet top-up",
                    ));
                }
                Some(provider_payment_id)
            }
            PaymentVerification::Pending { .. } => {
                return Err(ApiError::conflict("payment is still pending"));
            }
            PaymentVerification::Failed { .. } => return Err(ApiError::conflict("payment failed")),
        },
    };
    let payment_id = provider_payment_id
        .ok_or_else(|| ApiError::internal("completed top-up has no provider payment id"))?;
    let completed = if session.status == WalletTopUpSession::COMPLETED {
        session
    } else {
        state
            .wallet_repository
            .complete_top_up_session(tenant_id, &provider_checkout_id, &payment_id)
            .await
            .map_err(|e| ApiError::internal_with_source("failed to complete wallet top-up", e))?
            .ok_or_else(|| ApiError::not_found("wallet top-up session not found"))?
    };
    state
        .wallet_repository
        .credit(
            tenant_id,
            completed.amount_units,
            domain::WalletTransactionType::TopUp,
            &format!("topup:{payment_id}"),
            Some("Wallet top-up"),
        )
        .await
        .map_err(|e| ApiError::internal_with_source("failed to credit wallet", e))?;
    Ok(Json(TopUpResponse {
        checkout_url: String::new(),
        provider_checkout_id: completed.provider_checkout_id,
        amount_units: completed.amount_units,
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
        daily_spend_limit_units: req.daily_spend_limit_units,
        monthly_spend_limit_units: req.monthly_spend_limit_units,
        daily_usage_alert_threshold_units: req.daily_usage_alert_threshold_units,
        monthly_usage_alert_threshold_units: req.monthly_usage_alert_threshold_units,
        retry_limit: req.retry_limit,
    };
    if s.critical_balance_threshold_units > s.low_balance_threshold_units
        || s.low_balance_threshold_units < 0
        || s.auto_top_up_threshold_units < 0
        || s.auto_top_up_amount_units < domain::MIN_PAYG_TOP_UP_UNITS
        || s.daily_top_up_limit_units < s.auto_top_up_amount_units
        || s.monthly_top_up_limit_units < s.auto_top_up_amount_units
        || s.monthly_top_up_limit_units < s.daily_top_up_limit_units
        || s.daily_spend_limit_units.is_some_and(|v| v <= 0)
        || s.monthly_spend_limit_units.is_some_and(|v| v <= 0)
        || s.daily_usage_alert_threshold_units.is_some_and(|v| v <= 0)
        || s.monthly_usage_alert_threshold_units
            .is_some_and(|v| v <= 0)
        || s.monthly_spend_limit_units
            .zip(s.daily_spend_limit_units)
            .is_some_and(|(m, d)| m < d)
        || s.monthly_usage_alert_threshold_units
            .zip(s.daily_usage_alert_threshold_units)
            .is_some_and(|(m, d)| m < d)
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
