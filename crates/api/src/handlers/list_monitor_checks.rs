use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::info;

use application::use_cases::ListMonitorChecksUseCase;

use crate::{error::ApiError, state::AppState, validation::parse_monitor_id};

/// Lists all checks for a monitor.
pub async fn list_monitor_checks(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_monitor_checks");

    let monitor_id = match parse_monitor_id(&id) {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let use_case = ListMonitorChecksUseCase::new(state.check_repository.clone());

    match use_case.execute(monitor_id).await {
        Ok(checks) => (StatusCode::OK, Json(checks)).into_response(),
        Err(err) => ApiError::internal(format!("failed to list checks: {err}")).into_response(),
    }
}
