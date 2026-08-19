use axum::{
    extract::{Extension, Json, Path, State},
    http::StatusCode,
};
use tracing::info;

use application::use_cases::{CreateApiFlowError, CreateApiFlowUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::flow_types::{ApiFlowHttpResponse, CreateApiFlowHttpRequest},
    state::AppState,
    validation::parse_monitor_id,
};

pub async fn create_api_flow(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(payload): Json<CreateApiFlowHttpRequest>,
) -> Result<(StatusCode, Json<ApiFlowHttpResponse>), ApiError> {
    info!("API: create_api_flow");

    let monitor_id = parse_monitor_id(&id)?;

    // Authorization check: the monitor must belong to the authenticated user.
    state
        .monitor_repository
        .find_by_id_for_user(monitor_id, authenticated_user.user_id())
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch monitor", err))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    let request = payload
        .try_into_request(monitor_id)
        .map_err(ApiError::bad_request)?;

    let use_case = CreateApiFlowUseCase::new(
        state.api_flow_repository.clone(),
        state.id_generator,
        state.audit_log_entry_repository.clone(),
    );

    let flow = use_case
        .execute(request, authenticated_user)
        .await
        .map_err(|err| match err {
            CreateApiFlowError::FlowAlreadyExists => {
                ApiError::conflict("an api flow already exists for this monitor")
            }
            CreateApiFlowError::Other(err) => {
                ApiError::internal_with_source("failed to create api flow", err)
            }
        })?;

    Ok((StatusCode::CREATED, Json(ApiFlowHttpResponse::from(flow))))
}
