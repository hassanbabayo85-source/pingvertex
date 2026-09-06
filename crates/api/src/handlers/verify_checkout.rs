use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};

use application::{
    commands::{VerifyCheckoutCommand, VerifyCheckoutOutcome},
    use_cases::VerifyCheckoutUseCase,
};
use domain::shared::ids::CheckoutSessionId;
use domain::shared::types::AuthenticatedUser;

use crate::error::ApiError;
use crate::handlers::billing_types::{VerifyCheckoutRequest, VerifyCheckoutResponse};
use crate::handlers::initiate_checkout::billing_error_to_api_error;
use crate::state::AppState;

pub async fn verify_checkout(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<VerifyCheckoutRequest>,
) -> Result<(StatusCode, Json<VerifyCheckoutResponse>), ApiError> {
    // IMPORTANT: the client only supplies the internal checkout session
    // id. The server loads that session and asks the provider directly
    // for payment status — client-reported payment status is never used.
    let checkout_session_id = CheckoutSessionId(payload.checkout_session_id);

    let use_case = VerifyCheckoutUseCase::new(
        state.checkout_session_repository.clone(),
        state.subscription_repository.clone(),
        state.payment_provider.clone(),
    );

    let outcome = use_case
        .execute(VerifyCheckoutCommand {
            checkout_session_id,
            tenant_id: authenticated_user.tenant_context().tenant_id(),
        })
        .await
        .map_err(billing_error_to_api_error)?;

    let audit_action = match outcome.checkout_status {
        domain::CheckoutStatus::Completed => Some("payment_verified"),
        domain::CheckoutStatus::Failed => Some("payment_failed"),
        domain::CheckoutStatus::Pending => None,
    };
    if let Some(action) = audit_action {
        let _ = state
            .financial_audit_log_repository
            .create(&domain::FinancialAuditLogEntry::new(
                authenticated_user.user_id().0.to_string(),
                Some(authenticated_user.tenant_context().tenant_id().0),
                action,
                "checkout_session",
                Some(checkout_session_id.0.to_string()),
                serde_json::json!({"status": outcome.checkout_status.as_str()}),
            ))
            .await;
    }

    Ok((
        StatusCode::OK,
        Json(VerifyCheckoutResponse::from_outcome(outcome)),
    ))
}

impl VerifyCheckoutResponse {
    fn from_outcome(outcome: VerifyCheckoutOutcome) -> Self {
        Self {
            status: outcome.checkout_status.as_str().to_owned(),
        }
    }
}
