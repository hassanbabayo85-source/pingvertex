use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};

use application::{
    commands::{InitiateCheckoutCommand, InitiateCheckoutOutcome},
    use_cases::InitiateCheckoutUseCase,
    use_cases::billing_support::BillingError,
};
use domain::enums::PlanTier;
use domain::shared::types::AuthenticatedUser;

use crate::error::ApiError;
use crate::handlers::billing_types::{InitiateCheckoutRequest, InitiateCheckoutResponse};
use crate::state::AppState;

pub async fn initiate_checkout(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<InitiateCheckoutRequest>,
) -> Result<(StatusCode, Json<InitiateCheckoutResponse>), ApiError> {
    let plan_tier: PlanTier = payload
        .plan_tier
        .parse()
        .map_err(|_| ApiError::bad_request("invalid plan tier"))?;

    let use_case = InitiateCheckoutUseCase::new(
        state.checkout_session_repository.clone(),
        state.payment_provider.clone(),
    );

    let outcome = use_case
        .execute(InitiateCheckoutCommand {
            tenant_id: authenticated_user.tenant_context().tenant_id(),
            plan_tier,
        })
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
