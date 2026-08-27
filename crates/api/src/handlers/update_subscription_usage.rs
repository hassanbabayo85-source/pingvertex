use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, State},
};
use tracing::info;

use application::ports::IdGenerator;
use application::use_cases::update_subscription_usage::{
    UpdateSubscriptionUsageCommand, UpdateSubscriptionUsageError, UpdateSubscriptionUsageUseCase,
};
use domain::entities::Plan;
use domain::enums::PlanTier;
use domain::shared::types::{AuthenticatedUser, Permission};

use crate::{error::ApiError, handlers::subscription_types::*, state::AppState};

fn require_manage_billing(user: &AuthenticatedUser) -> Result<(), ApiError> {
    if user.can(Permission::ManageBilling) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "you do not have permission to manage this tenant's subscription usage",
        ))
    }
}

fn usage_error_to_api_error(err: UpdateSubscriptionUsageError) -> ApiError {
    match err {
        UpdateSubscriptionUsageError::SubscriptionNotFound => {
            ApiError::not_found("no subscription found for this tenant")
        }
        UpdateSubscriptionUsageError::InvalidUsage(domain_err) => {
            ApiError::bad_request(domain_err.to_string())
        }
        UpdateSubscriptionUsageError::Other(e) => {
            ApiError::internal_with_source("failed to update subscription usage", e)
        }
    }
}

/// H12 — Proration Service: `PATCH /api/v1/subscription/usage`.
///
/// Changes the caller's tenant subscription usage snapshot
/// (monitors/interval_seconds/retention_days) mid-cycle, closing the
/// currently-open `SubscriptionUsagePeriod` and opening a new one so
/// billing can later prorate correctly across both
/// (`CalculateSubscriptionCycleCostUseCase`). Usage values are derived
/// from `Plan::for_tier`, keyed by the plan tier the client selects —
/// never taken as raw numbers from the client.
pub async fn update_subscription_usage(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<UpdateSubscriptionUsageRequest>,
) -> Result<Json<UpdateSubscriptionUsageResponse>, ApiError> {
    info!("API: update_subscription_usage");
    require_manage_billing(&authenticated_user)?;

    let plan_tier: PlanTier = payload
        .plan_tier
        .parse()
        .map_err(|_| ApiError::bad_request("invalid plan tier"))?;

    let plan = Plan::for_tier(plan_tier);

    let pricing_config = state
        .pricing_config_repository
        .find_active_config()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch active pricing config", e))?
        .ok_or_else(|| ApiError::internal("no active pricing config found"))?;

    let id_generator: Arc<dyn IdGenerator> = Arc::new(state.id_generator.clone());

    let use_case = UpdateSubscriptionUsageUseCase::new(
        state.subscription_repository.clone(),
        state.subscription_usage_period_repository.clone(),
        id_generator,
        state.subscription_usage_transaction.clone(),
    );

    use_case
        .execute(UpdateSubscriptionUsageCommand {
            tenant_id: authenticated_user.tenant_context().tenant_id(),
            monitors: plan.max_monitors as u64,
            interval_seconds: plan.min_check_interval_seconds,
            retention_days: plan.history_retention_days,
            pricing_version: pricing_config.version,
        })
        .await
        .map_err(usage_error_to_api_error)?;

    Ok(Json(UpdateSubscriptionUsageResponse {
        plan_tier: payload.plan_tier,
        monitors: plan.max_monitors as u64,
        interval_seconds: plan.min_check_interval_seconds,
        retention_days: plan.history_retention_days,
        pricing_version: pricing_config.version,
    }))
}
