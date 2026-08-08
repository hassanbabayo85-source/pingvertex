use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::info;

use application::use_cases::ListIncidentsUseCase;

use crate::{error::ApiError, state::AppState, validation::parse_monitor_id};

/// Lists all incidents for a monitor.
pub async fn list_incidents(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_incidents");

    let monitor_id = match parse_monitor_id(&id) {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let use_case = ListIncidentsUseCase::new(state.incident_repository.clone());

    match use_case.execute(monitor_id).await {
        Ok(incidents) => (StatusCode::OK, Json(incidents)).into_response(),
        Err(err) => ApiError::internal(format!("failed to list incidents: {err}")).into_response(),
    }
}
