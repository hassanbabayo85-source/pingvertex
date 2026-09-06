use crate::{error::ApiError, state::AppState};
use application::use_cases::change_subscription_plan::{
    ChangePlanError, ChangeSubscriptionPlanUseCase,
};
use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use domain::PlanTier;
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Debug, Deserialize)]
pub struct ChangePlanRequest {
    pub plan_tier: String,
}
#[derive(Debug, Deserialize)]
pub struct VerifyPlanChangeRequest {
    pub checkout_session_id: uuid::Uuid,
}
#[derive(Debug, Serialize)]
pub struct ChangePlanResponse {
    pub plan_change_id: uuid::Uuid,
    pub calculated_amount: String,
    pub checkout_session_id: Option<uuid::Uuid>,
    pub checkout_url: Option<String>,
    pub applied: bool,
}
fn auth(u: &AuthenticatedUser) -> Result<(), ApiError> {
    if u.can(Permission::ManageBilling) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "you do not have permission to change this subscription",
        ))
    }
}
fn err(e: ChangePlanError) -> ApiError {
    match e {
        ChangePlanError::SubscriptionNotFound => ApiError::not_found("subscription not found"),
        ChangePlanError::SamePlan => ApiError::conflict("requested plan is already active"),
        ChangePlanError::Invalid(s) => ApiError::bad_request(s),
        ChangePlanError::Pricing(e)
        | ChangePlanError::Provider(e)
        | ChangePlanError::Repository(e) => {
            ApiError::internal_with_source("subscription plan change failed", e)
        }
    }
}
pub async fn change_plan(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Json(r): Json<ChangePlanRequest>,
) -> Result<(StatusCode, Json<ChangePlanResponse>), ApiError> {
    auth(&u)?;
    let tier = r
        .plan_tier
        .parse::<PlanTier>()
        .map_err(|_| ApiError::bad_request("invalid plan tier"))?;
    let ids: Arc<dyn application::ports::IdGenerator> = Arc::new(s.id_generator.clone());
    let uc = ChangeSubscriptionPlanUseCase::new(
        s.subscription_repository.clone(),
        s.pricing_config_repository.clone(),
        s.subscription_plan_change_repository.clone(),
        s.checkout_session_repository.clone(),
        s.payment_provider.clone(),
        s.wallet_repository.clone(),
        s.subscription_usage_period_repository.clone(),
        s.subscription_usage_transaction.clone(),
        ids,
    );
    let o = uc
        .execute(u.tenant_context().tenant_id(), tier)
        .await
        .map_err(err)?;
    let _ = s
        .financial_audit_log_repository
        .create(&domain::FinancialAuditLogEntry::new(
            u.user_id().0.to_string(),
            Some(u.tenant_context().tenant_id().0),
            "subscription_changed",
            "subscription_plan_change",
            Some(o.plan_change_id.to_string()),
            serde_json::json!({"new_plan":tier.as_str(),"amount":o.calculated_amount.to_string()}),
        ))
        .await;
    Ok((
        StatusCode::OK,
        Json(ChangePlanResponse {
            plan_change_id: o.plan_change_id,
            calculated_amount: o.calculated_amount.to_string(),
            checkout_session_id: o.checkout_session_id,
            checkout_url: o.checkout_url,
            applied: o.applied,
        }),
    ))
}
pub async fn verify_plan_change(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
    Json(r): Json<VerifyPlanChangeRequest>,
) -> Result<Json<ChangePlanResponse>, ApiError> {
    auth(&u)?;
    let ids: Arc<dyn application::ports::IdGenerator> = Arc::new(s.id_generator.clone());
    let uc = ChangeSubscriptionPlanUseCase::new(
        s.subscription_repository.clone(),
        s.pricing_config_repository.clone(),
        s.subscription_plan_change_repository.clone(),
        s.checkout_session_repository.clone(),
        s.payment_provider.clone(),
        s.wallet_repository.clone(),
        s.subscription_usage_period_repository.clone(),
        s.subscription_usage_transaction.clone(),
        ids,
    );
    let o = uc
        .apply_verified_change(u.tenant_context().tenant_id(), r.checkout_session_id)
        .await
        .map_err(err)?;
    if o.applied {
        let _ = s
            .financial_audit_log_repository
            .create(&domain::FinancialAuditLogEntry::new(
                u.user_id().0.to_string(),
                Some(u.tenant_context().tenant_id().0),
                "subscription_changed",
                "subscription_plan_change",
                Some(o.plan_change_id.to_string()),
                serde_json::json!({"verified":true}),
            ))
            .await;
    }
    Ok(Json(ChangePlanResponse {
        plan_change_id: o.plan_change_id,
        calculated_amount: o.calculated_amount.to_string(),
        checkout_session_id: o.checkout_session_id,
        checkout_url: o.checkout_url,
        applied: o.applied,
    }))
}
