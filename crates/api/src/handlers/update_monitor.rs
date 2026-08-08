use axum::{
    extract::{Extension, Path, State},
    Json,
};
use tracing::info;

use application::{commands::UpdateMonitor, use_cases::UpdateMonitorUseCase};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError,
    handlers::types::{MonitorResponse, MonitorWriteRequest},
    state::AppState,
    validation::parse_monitor_id,
};

pub async fn update_monitor(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<String>,
    Json(body): Json<MonitorWriteRequest>,
) -> Result<Json<MonitorResponse>, ApiError> {
    info!("API: update_monitor");

    let monitor_id = parse_monitor_id(&id)?;

    let command = UpdateMonitor {
        id: monitor_id,
        user_id,
        name: body.name,
        url: body.url,
        method: body.method,
        expected_status: body.expected_status,
        interval_seconds: body.interval_seconds,
        timeout_seconds: body.timeout_seconds,
    };

    let use_case = UpdateMonitorUseCase::new(state.monitor_repository.clone());

    let monitor = use_case
        .execute(command)
        .await
        .map_err(|err| ApiError::internal(format!("failed to update monitor: {err}")))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    Ok(Json(MonitorResponse::from(monitor)))
}
