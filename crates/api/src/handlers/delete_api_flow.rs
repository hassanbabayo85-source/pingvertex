use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tracing::info;

use application::{commands::DeleteApiFlow, use_cases::DeleteApiFlowUseCase};
use domain::shared::ids::ApiFlowId;
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    state::AppState,
    validation::{parse_monitor_id, parse_uuid},
};

pub async fn delete_api_flow(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path((id, flow_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    info!("API: delete_api_flow");

    let monitor_id = parse_monitor_id(&id)?;
    let flow_id = ApiFlowId(parse_uuid(&flow_id)?);

    // Authorization check: the monitor must belong to the authenticated user.
    state
        .monitor_repository
        .find_by_id_for_user(monitor_id, authenticated_user.user_id())
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch monitor", err))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    let use_case = DeleteApiFlowUseCase::new(
        state.api_flow_repository.clone(),
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let deleted = use_case
        .execute(
            DeleteApiFlow {
                id: flow_id,
                monitor_id,
            },
            authenticated_user,
        )
        .await
        .map_err(|err| ApiError::internal_with_source("failed to delete api flow", err))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("api flow not found"))
    }
}
