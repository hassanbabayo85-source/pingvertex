use crate::{error::ApiError, state::AppState, validation::parse_monitor_id};
use axum::{
    Json,
    extract::{Extension, Path, State},
};
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::Serialize;
use uuid::Uuid;
#[derive(Debug, Serialize)]
pub struct OneTimeCheckResponse {
    pub id: String,
    pub request_id: String,
    pub status: String,
}
pub async fn create_one_time_check(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<OneTimeCheckResponse>, ApiError> {
    user.require(Permission::ManageBilling)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    let monitor_id = parse_monitor_id(&id)?;
    let tenant_id = user.tenant_context().tenant_id();
    let monitor = state
        .monitor_repository
        .find_by_id(monitor_id)
        .await
        .map_err(|_| ApiError::internal("monitor lookup failed"))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;
    if monitor.tenant_id != tenant_id {
        return Err(ApiError::not_found("monitor not found"));
    }
    if monitor.kind == domain::MonitorKind::Heartbeat {
        return Err(ApiError::bad_request(
            "heartbeat monitors are passive and cannot run a one-time active check",
        ));
    }
    state
        .wallet_repository
        .get_or_create(tenant_id.0, "USD")
        .await
        .map_err(|_| ApiError::internal("wallet lookup failed"))?;
    if state
        .wallet_repository
        .get_balance_summary(tenant_id.0, "USD")
        .await
        .map_err(|_| ApiError::internal("wallet lookup failed"))?
        .available_units
        <= 0
    {
        return Err(ApiError::payment_required(
            "available PAYG wallet credit is required for a one-time check",
            "PAYG_INSUFFICIENT_BALANCE",
        ));
    }
    let request_id = format!("one-time-check:{}", Uuid::new_v4());
    let r = state
        .scheduling_repository
        .enqueue_one_time_check(tenant_id, monitor_id, user.user_id(), &request_id)
        .await
        .map_err(|_| ApiError::internal("failed to queue one-time check"))?;
    Ok(Json(OneTimeCheckResponse {
        id: r.id.to_string(),
        request_id: r.request_id,
        status: r.status,
    }))
}
pub async fn get_one_time_check(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((monitor_id, request_id)): Path<(String, String)>,
) -> Result<Json<OneTimeCheckResponse>, ApiError> {
    user.require(Permission::ViewMonitor)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    let monitor_id = parse_monitor_id(&monitor_id)?;
    let request_id = Uuid::parse_str(&request_id)
        .map_err(|_| ApiError::bad_request("invalid one-time check id"))?;
    let r = state
        .scheduling_repository
        .get_one_time_check(user.tenant_context().tenant_id(), monitor_id, request_id)
        .await
        .map_err(|_| ApiError::internal("one-time check lookup failed"))?
        .ok_or_else(|| ApiError::not_found("one-time check not found"))?;
    Ok(Json(OneTimeCheckResponse {
        id: r.id.to_string(),
        request_id: r.request_id,
        status: r.status,
    }))
}
