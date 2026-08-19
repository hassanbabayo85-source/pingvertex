use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::info;

use application::use_cases::{ListMonitorChecksError, ListMonitorChecksUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, state::AppState, validation::parse_monitor_id};

/// Lists all checks for a monitor (any user in the monitor's tenant).
pub async fn list_monitor_checks(
    Path(id): Path<String>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_monitor_checks");

    let monitor_id = match parse_monitor_id(&id) {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let use_case = ListMonitorChecksUseCase::new(
        state.monitor_repository.clone(),
        state.check_repository.clone(),
    );

    match use_case.execute(monitor_id, authenticated_user).await {
        Ok(checks) => (StatusCode::OK, Json(checks)).into_response(),
        Err(ListMonitorChecksError::NotFound) => {
            ApiError::not_found("monitor not found").into_response()
        }
        Err(ListMonitorChecksError::Other(err)) => {
            ApiError::internal_with_source("failed to list checks", err).into_response()
        }
    }
}
