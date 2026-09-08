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
/// Closes the tenant's currently-open `SubscriptionUsagePeriod` and
/// opens a new one, so billing can later prorate correctly across both
/// (`CalculateSubscriptionCycleCostUseCase`).
///
/// SECURITY (P0 — fixed): the usage values written here (monitors,
/// interval_seconds, retention_days) are derived ENTIRELY inside the
/// use case from the tenant's own `Subscription::plan_tier`. This
/// handler does not accept, parse, or forward a client-supplied plan
/// tier or usage numbers — only the caller's tenant id and the
/// currently active pricing config version are passed through.
#[allow(unreachable_code, unused_variables)]
pub async fn update_subscription_usage(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(_payload): Json<UpdateSubscriptionUsageRequest>,
) -> Result<Json<UpdateSubscriptionUsageResponse>, ApiError> {
    return Err(ApiError::gone(
        "subscription usage is retired; use PAYG wallet billing",
    ));

    info!("API: update_subscription_usage");
    require_manage_billing(&authenticated_user)?;

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

    let outcome = use_case
        .execute(UpdateSubscriptionUsageCommand {
            tenant_id: authenticated_user.tenant_context().tenant_id(),
            pricing_version: pricing_config.version,
        })
        .await
        .map_err(usage_error_to_api_error)?;

    Ok(Json(UpdateSubscriptionUsageResponse {
        plan_tier: outcome.plan_tier.as_str().to_string(),
        monitors: outcome.monitors,
        interval_seconds: outcome.interval_seconds,
        retention_days: outcome.retention_days,
        pricing_version: outcome.pricing_version,
    }))
}
