use application::ports::{PricingContext, resolve_price};
use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Deserialize)]
pub struct CreateOneTimeCheckRequest {
    pub monitor_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct OneTimeCheckResponse {
    pub request_id: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<String>,
}

pub async fn create_one_time_check(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateOneTimeCheckRequest>,
) -> Result<(StatusCode, Json<OneTimeCheckResponse>), ApiError> {
    user.require(Permission::CreateMonitor).map_err(|_| {
        ApiError::forbidden("you do not have permission to trigger one-time checks")
    })?;

    info!("API: create_one_time_check");

    let tenant_id = user.tenant_context().tenant_id();
    let monitor_id = domain::shared::ids::MonitorId(payload.monitor_id);

    let monitor = state
        .monitor_repository
        .find_by_id(monitor_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch monitor", err))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    if monitor.tenant_id != tenant_id {
        return Err(ApiError::not_found("monitor not found"));
    }

    let operation = domain::monitor_operation(monitor.kind, true);
    let pricing_context = PricingContext {
        quantity: 1,
        monitor_kind: Some(monitor.kind),
        region: monitor.regions.first().cloned(),
        account_type: None, // In production, we'd query tenant repo here or pass via enriched context
    };

    let price = resolve_price(
        state.payg_pricing_repository.as_ref(),
        operation.as_str(),
        &pricing_context,
    )
    .await
    .map_err(|e| ApiError::internal_with_source("failed to resolve PAYG price", e))?;

    if price.is_none() {
        return Err(ApiError::bad_request(
            "PAYG pricing is not configured for one-time checks",
        ));
    }

    let request_id = Uuid::new_v4().to_string();

    let request = state
        .scheduling_repository
        .enqueue_one_time_check(tenant_id, monitor_id, user.user_id(), &request_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to enqueue one-time check", err))?;

    Ok((
        StatusCode::CREATED,
        Json(OneTimeCheckResponse {
            request_id: request.request_id,
            status: request.status,
            created_at: request.created_at,
            completed_at: request.completed_at,
            error: request.error,
        }),
    ))
}

pub async fn get_one_time_check(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    axum::extract::Path(request_id): axum::extract::Path<String>,
) -> Result<Json<OneTimeCheckResponse>, ApiError> {
    user.require(Permission::ViewMonitor)
        .map_err(|_| ApiError::forbidden("you do not have permission to view one-time checks"))?;

    let request = state
        .scheduling_repository
        .get_one_time_check_by_request_id(
            user.tenant_context().tenant_id(),
            domain::shared::ids::MonitorId(Uuid::nil()), // Ignored by updated query
            &request_id,
        )
        .await
        .map_err(|err| ApiError::internal_with_source("failed to get one-time check", err))?
        .ok_or_else(|| ApiError::not_found("one-time check not found"))?;

    Ok(Json(OneTimeCheckResponse {
        request_id: request.request_id,
        status: request.status,
        created_at: request.created_at,
        completed_at: request.completed_at,
        error: request.error,
    }))
}
