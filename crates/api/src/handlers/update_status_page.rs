use axum::{
    extract::{Extension, Path, State},
    Json,
};
use tracing::info;

use application::{
    commands::UpdateStatusPage,
    use_cases::{UpdateStatusPageError, UpdateStatusPageUseCase},
};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::{StatusPageResponse, StatusPageUpdateRequest},
    state::AppState,
    validation::{parse_monitor_id, parse_status_page_id},
};

pub async fn update_status_page(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(payload): Json<StatusPageUpdateRequest>,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: update_status_page");

    let status_page_id = parse_status_page_id(&id)?;
    let monitor_ids = payload
        .monitor_ids
        .iter()
        .map(|id| parse_monitor_id(id))
        .collect::<Result<Vec<_>, _>>()?;

    let command = UpdateStatusPage {
        id: status_page_id,
        name: payload.name,
        monitor_ids,
        logo_url: payload.logo_url,
        primary_color: payload.primary_color,
    };

    let use_case = UpdateStatusPageUseCase::new(
        state.status_page_repository.clone(),
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let status_page = use_case
        .execute(command, authenticated_user)
        .await
        .map_err(|err| match err {
            UpdateStatusPageError::MonitorNotFound { .. } => {
                ApiError::not_found("monitor not found")
            }
            UpdateStatusPageError::Other(inner) => {
                ApiError::internal_with_source("failed to update status page", inner)
            }
        })?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    Ok(Json(StatusPageResponse::from(status_page)))
}
