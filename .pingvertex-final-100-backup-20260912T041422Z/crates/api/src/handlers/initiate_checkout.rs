use crate::error::ApiError;
use crate::handlers::billing_types::{InitiateCheckoutRequest, InitiateCheckoutResponse};
use crate::state::AppState;
use application::{
    commands::{InitiateCheckoutCommand, InitiateCheckoutOutcome},
    use_cases::{
        InitiateCheckoutUseCase, billing_support::BillingError,
        calculate_pricing_quote::CalculatePricingQuoteUseCase,
    },
};
use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use domain::pricing::PricingInput;
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{Plan, PlanTier};
use rust_decimal::prelude::ToPrimitive;

#[allow(unreachable_code, unused_variables)]
pub async fn initiate_checkout(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<InitiateCheckoutRequest>,
) -> Result<(StatusCode, Json<InitiateCheckoutResponse>), ApiError> {
    return Err(ApiError::gone(
        "subscription checkout is retired; use PAYG wallet top-up",
    ));

    authenticated_user
        .require(Permission::ManageBilling)
        .map_err(|_| ApiError::forbidden("billing management is not permitted"))?;
    let plan_tier: PlanTier = payload
        .plan_tier
        .parse()
        .map_err(|_| ApiError::bad_request("invalid plan tier"))?;
    let plan = Plan::for_tier(plan_tier);
    let quote = CalculatePricingQuoteUseCase::new(state.pricing_config_repository.clone())
        .execute(PricingInput {
            monitors: plan.max_monitors as u64,
            interval_seconds: plan.min_check_interval_seconds,
            retention_days: plan.history_retention_days,
        })
        .await
        .map_err(|e| ApiError::internal_with_source("failed to calculate checkout amount", e))?;
    let amount_minor = (quote.monthly_price * rust_decimal::Decimal::from(100i64))
        .round_dp(0)
        .to_i64()
        .ok_or_else(|| ApiError::internal("checkout amount is out of range"))?;
    let use_case = InitiateCheckoutUseCase::new(
        state.checkout_session_repository.clone(),
        state.payment_provider.clone(),
    );
    let outcome = use_case
        .execute_with_amount(
            InitiateCheckoutCommand {
                tenant_id: authenticated_user.tenant_context().tenant_id(),
                plan_tier,
            },
            amount_minor,
            quote.currency.clone(),
            quote.monthly_price,
        )
        .await
        .map_err(billing_error_to_api_error)?;
    Ok((
        StatusCode::OK,
        Json(InitiateCheckoutResponse::from_outcome(outcome)),
    ))
}
impl InitiateCheckoutResponse {
    fn from_outcome(outcome: InitiateCheckoutOutcome) -> Self {
        Self {
            checkout_session_id: outcome.checkout_session_id.0.to_string(),
            checkout_url: outcome.checkout_url,
        }
    }
}
pub(crate) fn billing_error_to_api_error(err: BillingError) -> ApiError {
    match err {
        BillingError::FreePlanNotAllowed => {
            ApiError::bad_request("free plan does not require checkout")
        }
        BillingError::Provider(err) => {
            ApiError::internal_with_source("billing provider error", err)
        }
        BillingError::CheckoutSessionNotFound => ApiError::not_found("checkout session not found"),
        BillingError::CheckoutNotReady => ApiError::conflict("checkout session is not ready"),
        BillingError::Subscription(err) => {
            ApiError::internal_with_source("failed to activate subscription", err)
        }
    }
}
