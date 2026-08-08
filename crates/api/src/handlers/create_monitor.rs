use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use tracing::info;

use application::{dto::CreateMonitorRequest, use_cases::CreateMonitorUseCase};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError,
    handlers::types::{MonitorResponse, MonitorWriteRequest},
    state::AppState,
};

/// Creates a monitor through the API.
pub async fn create_monitor(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Json(payload): Json<MonitorWriteRequest>,
) -> Result<(StatusCode, Json<MonitorResponse>), ApiError> {
    info!("API: create_monitor");

    let request = CreateMonitorRequest {
        user_id,
        name: payload.name,
        url: payload.url,
        method: payload.method,
        expected_status: payload.expected_status,
        interval_seconds: payload.interval_seconds,
        timeout_seconds: payload.timeout_seconds,
    };

    let use_case = CreateMonitorUseCase::new(state.monitor_repository.clone(), state.id_generator);

    let monitor = use_case
        .execute(request)
        .await
        .map_err(|err| ApiError::internal(format!("failed to create monitor: {err}")))?;

    Ok((StatusCode::CREATED, Json(MonitorResponse::from(monitor))))
}
