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

    let request = payload
        .try_into_request(monitor_id)
        .map_err(ApiError::bad_request)?;

    // Authorization (monitor exists + belongs to the actor's tenant) is
    // enforced inside CreateApiFlowUseCase itself now, not here — see
    // that use case for why. This handler just wires the dependency in.
    let use_case = CreateApiFlowUseCase::new(
        state.api_flow_repository.clone(),
        state.id_generator,
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let flow = use_case
        .execute(request, authenticated_user)
        .await
        .map_err(|err| match err {
            CreateApiFlowError::MonitorNotFound => ApiError::not_found("monitor not found"),
            CreateApiFlowError::FlowAlreadyExists => {
                ApiError::conflict("an api flow already exists for this monitor")
            }
            CreateApiFlowError::Other(err) => {
                ApiError::internal_with_source("failed to create api flow", err)
            }
        })?;

    Ok((StatusCode::CREATED, Json(ApiFlowHttpResponse::from(flow))))
}
