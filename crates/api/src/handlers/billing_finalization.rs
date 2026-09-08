use crate::{error::ApiError, state::AppState};
use application::{commands::CancelSubscription, use_cases::CancelSubscriptionUseCase};
use axum::{
    Json,
    extract::{Extension, Path, State},
};
use domain::pricing::{PricingInput, calculate_price};
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{Plan, PlanTier, Subscription};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct SubscriptionResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub plan_tier: String,
    pub status: String,
    pub trial_ends_at: Option<chrono::DateTime<chrono::Utc>>,
    pub grace_period_ends_at: Option<chrono::DateTime<chrono::Utc>>,
    pub current_period_ends_at: Option<chrono::DateTime<chrono::Utc>>,
    pub canceled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub cancel_at_period_end: bool,
    pub monitors: Option<u64>,
    pub interval_seconds: Option<u32>,
    pub retention_days: Option<u32>,
    pub pricing_version: Option<u32>,
}
impl From<Subscription> for SubscriptionResponse {
    fn from(s: Subscription) -> Self {
        Self {
            id: s.id.0,
            tenant_id: s.tenant_id.0,
            plan_tier: s.plan_tier.as_str().into(),
            status: s.status.as_str().into(),
            trial_ends_at: s.trial_ends_at,
            grace_period_ends_at: s.grace_period_ends_at,
            current_period_ends_at: s.current_period_ends_at,
            canceled_at: s.canceled_at,
            cancel_at_period_end: s.cancel_at_period_end,
            monitors: s.monitors,
            interval_seconds: s.interval_seconds,
            retention_days: s.retention_days,
            pricing_version: s.pricing_version,
        }
    }
}

#[allow(unreachable_code, unused_variables)]
pub async fn get_subscription(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
) -> Result<Json<Option<SubscriptionResponse>>, ApiError> {
    return Err(ApiError::gone(
        "subscription billing is retired; use PAYG wallet billing",
    ));

    u.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    let tenant = u.tenant_context().tenant_id();
    let sub = s
        .subscription_repository
        .find_by_tenant_id(tenant)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load subscription", e))?;
    Ok(Json(sub.map(Into::into)))
}

#[derive(Debug, Deserialize)]
pub struct CancelSubscriptionRequest {
    pub at_period_end: bool,
    /// Ignored for security: the server derives the period end from the
    /// authenticated tenant's persisted subscription. Kept as an optional
    /// field so older clients do not fail deserialization.
    #[serde(default)]
    pub current_period_ends_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[allow(unreachable_code, unused_variables)]
pub async fn cancel_subscription(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Json(req): Json<CancelSubscriptionRequest>,
) -> Result<Json<SubscriptionResponse>, ApiError> {
    return Err(ApiError::gone(
        "subscription cancellation is retired; use PAYG wallet top-up",
    ));

    u.require(Permission::ManageBilling)
        .map_err(|_| ApiError::forbidden("billing management is not permitted"))?;
    let tenant = u.tenant_context().tenant_id();
    let uc = CancelSubscriptionUseCase::new(s.subscription_repository.clone());
    uc.execute(CancelSubscription {
        tenant_id: tenant,
        at_period_end: req.at_period_end,
        current_period_ends_at: None,
    })
    .await
    .map_err(|e| ApiError::internal_with_source("failed to cancel subscription", e))?;
    let sub = s
        .subscription_repository
        .find_by_tenant_id(tenant)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to reload subscription", e))?
        .ok_or_else(|| ApiError::internal("subscription disappeared after cancellation"))?;
    s.financial_audit_log_repository
        .create(&domain::FinancialAuditLogEntry::new(
            u.user_id().0.to_string(),
            Some(tenant.0),
            "subscription_canceled",
            "subscription",
            Some(sub.id.0.to_string()),
            serde_json::json!({"at_period_end":req.at_period_end}),
        ))
        .await
        .map_err(|e| ApiError::internal_with_source("failed to write financial audit log", e))?;
    Ok(Json(sub.into()))
}

#[derive(Debug, Serialize)]
pub struct PlanResponse {
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

#[allow(unreachable_code, unused_variables)]
pub async fn list_plans(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<PlanResponse>>, ApiError> {
    return Err(ApiError::gone(
        "subscription plans are retired; use PAYG pricing",
    ));

    u.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    let cfg = s
        .pricing_config_repository
        .find_active_config()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load pricing config", e))?
        .ok_or_else(|| ApiError::internal("pricing configuration missing"))?;
    // The active pricing config is already loaded above. Reuse it for every
    // plan instead of issuing one database lookup per tier (N+1 query pattern).
    let mut out = Vec::new();
    for tier in [
        PlanTier::Free,
        PlanTier::Starter,
        PlanTier::Pro,
        PlanTier::Business,
        PlanTier::Enterprise,
    ] {
        let p = Plan::for_tier(tier);
        let q = calculate_price(
            PricingInput {
                monitors: p.max_monitors as u64,
                interval_seconds: p.min_check_interval_seconds,
                retention_days: p.history_retention_days,
            },
            &cfg,
        )
        .map_err(|e| ApiError::internal_with_source("failed to calculate plan price", e))?;
        out.push(PlanResponse {
            tier: tier.as_str().into(),
            max_monitors: p.max_monitors,
            min_check_interval_seconds: p.min_check_interval_seconds,
            history_retention_days: p.history_retention_days,
            max_locations: p.max_locations,
            max_team_members: p.max_team_members,
            rum_enabled: p.rum_enabled,
            advanced_features: p.advanced_features,
            monthly_price: q.monthly_price.to_string(),
            currency: cfg.currency.clone(),
        });
    }
    Ok(Json(out))
}

pub async fn get_invoice(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::handlers::invoices::InvoiceResponse>, ApiError> {
    u.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    let tenant = u.tenant_context().tenant_id().0;
    let invoice = s
        .invoice_repository
        .find_by_id_for_tenant(id, tenant)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load invoice", e))?
        .ok_or_else(|| ApiError::not_found("invoice not found"))?;
    Ok(Json(invoice.into()))
}
