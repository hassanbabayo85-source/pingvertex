use axum::{
    extract::{Extension, Path, State},
    Json,
};
use tracing::info;

use application::{commands::UpdateStatusPage, use_cases::UpdateStatusPageUseCase};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError,
    handlers::types::{StatusPageResponse, StatusPageUpdateRequest},
    state::AppState,
    validation::{parse_monitor_id, parse_status_page_id},
};

pub async fn update_status_page(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<String>,
    Json(payload): Json<StatusPageUpdateRequest>,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: update_status_page");

    let id = parse_status_page_id(&id)?;

    let monitor_ids = payload
        .monitor_ids
        .iter()
        .map(|mid| parse_monitor_id(mid))
        .collect::<Result<Vec<_>, _>>()?;

    let command = UpdateStatusPage {
        id,
        user_id,
        name: payload.name,
        monitor_ids,
        logo_url: payload.logo_url,
        primary_color: payload.primary_color,
    };

    let use_case = UpdateStatusPageUseCase::new(state.status_page_repository.clone());

    let status_page = use_case
        .execute(command)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to update status page", err))?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    Ok(Json(StatusPageResponse::from(status_page)))
}
