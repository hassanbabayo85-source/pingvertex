use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};

use application::{
    commands::{VerifyCheckoutCommand, VerifyCheckoutOutcome},
    ports::CheckoutSessionRepository,
    use_cases::VerifyCheckoutUseCase,
};
use domain::shared::ids::CheckoutSessionId;
use domain::shared::types::{AuthenticatedUser, Permission};

use crate::error::ApiError;
use crate::handlers::billing_types::{VerifyCheckoutRequest, VerifyCheckoutResponse};
use crate::handlers::initiate_checkout::billing_error_to_api_error;
use crate::state::AppState;

pub async fn verify_checkout(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<VerifyCheckoutRequest>,
) -> Result<(StatusCode, Json<VerifyCheckoutResponse>), ApiError> {
    authenticated_user
        .require(Permission::ManageBilling)
        .map_err(|_| ApiError::forbidden("billing management is not permitted"))?;

    // IMPORTANT: the client only supplies the internal checkout session
    // id. The server loads that session and asks the provider directly
    // for payment status — client-reported payment status is never used.
    let checkout_session_id = CheckoutSessionId(payload.checkout_session_id);

    let tenant_id = authenticated_user.tenant_context().tenant_id();
    let checkout_before = state
        .checkout_session_repository
        .find_by_id(checkout_session_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load checkout session", e))?
        .ok_or_else(|| ApiError::not_found("checkout session not found"))?;
    if checkout_before.tenant_id != tenant_id {
        return Err(ApiError::not_found("checkout session not found"));
    }
    let subscription_before = state
        .subscription_repository
        .find_by_tenant_id(tenant_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load subscription", e))?;

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
    if checkout_before.status != domain::CheckoutStatus::Completed
        && let Some(action) = audit_action
    {
        state
            .financial_audit_log_repository
            .create(&domain::FinancialAuditLogEntry::new(
                authenticated_user.user_id().0.to_string(),
                Some(tenant_id.0),
                action,
                "checkout_session",
                Some(checkout_session_id.0.to_string()),
                serde_json::json!({"status": outcome.checkout_status.as_str()}),
            ))
            .await
            .map_err(|e| {
                ApiError::internal_with_source("failed to write financial audit log", e)
            })?;
    }
    if checkout_before.status != domain::CheckoutStatus::Completed
        && outcome.checkout_status == domain::CheckoutStatus::Completed
    {
        let action = if subscription_before.is_some() {
            "subscription_changed"
        } else {
            "subscription_created"
        };
        let subscription_id = state
            .subscription_repository
            .find_by_tenant_id(tenant_id)
            .await
            .map_err(|e| ApiError::internal_with_source("failed to reload subscription", e))?
            .map(|s| s.id.0.to_string());
        state
            .financial_audit_log_repository
            .create(&domain::FinancialAuditLogEntry::new(
                authenticated_user.user_id().0.to_string(),
                Some(tenant_id.0),
                action,
                "subscription",
                subscription_id,
                serde_json::json!({"checkout_session_id":checkout_session_id.0.to_string()}),
            ))
            .await
            .map_err(|e| {
                ApiError::internal_with_source("failed to write financial audit log", e)
            })?;
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
