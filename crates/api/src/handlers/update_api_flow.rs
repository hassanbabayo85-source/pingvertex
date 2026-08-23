use axum::extract::{Extension, Json, Path, State};
use tracing::info;

use application::{commands::UpdateApiFlow, use_cases::UpdateApiFlowUseCase};
use domain::shared::ids::ApiFlowId;
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::flow_types::{ApiFlowHttpResponse, CreateApiFlowHttpRequest},
    state::AppState,
    validation::{parse_monitor_id, parse_uuid},
};

pub async fn update_api_flow(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path((id, flow_id)): Path<(String, String)>,
    Json(payload): Json<CreateApiFlowHttpRequest>,
) -> Result<Json<ApiFlowHttpResponse>, ApiError> {
    info!("API: update_api_flow");

    let monitor_id = parse_monitor_id(&id)?;
    let flow_id = ApiFlowId(parse_uuid(&flow_id)?);

    // Authorization check: the monitor must belong to the authenticated user's tenant.
    let monitor = state
        .monitor_repository
        .find_by_id(monitor_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch monitor", err))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    if !AuthorizationPolicy::same_tenant(&authenticated_user.tenant_context(), monitor.tenant_id())
    {
        return Err(ApiError::not_found("monitor not found"));
    }

    let request = payload
        .try_into_request(monitor_id)
        .map_err(ApiError::bad_request)?;

    let use_case = UpdateApiFlowUseCase::new(
        state.api_flow_repository.clone(),
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let flow = use_case
        .execute(
            UpdateApiFlow {
                id: flow_id,
                monitor_id,
                steps: request.steps,
            },
            authenticated_user,
        )
        .await
        .map_err(|err| ApiError::internal_with_source("failed to update api flow", err))?
        .ok_or_else(|| ApiError::not_found("api flow not found"))?;

    Ok(Json(ApiFlowHttpResponse::from(flow)))
}
