use axum::{
    extract::{Extension, Path, State},
    Json,
};
use tracing::info;

use application::{queries::GetMonitor, use_cases::GetMonitorUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError, handlers::types::MonitorResponse, state::AppState,
    validation::parse_monitor_id,
};

/// Gets a single monitor through the API.
pub async fn get_monitor(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<MonitorResponse>, ApiError> {
    info!("API: get_monitor");

    let id = parse_monitor_id(&id)?;

    let use_case = GetMonitorUseCase::new(state.monitor_repository.clone());

    let monitor = use_case
        .execute(GetMonitor { id }, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch monitor", err))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    Ok(Json(MonitorResponse::from(monitor)))
}
