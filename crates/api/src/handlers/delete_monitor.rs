use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tracing::info;

use application::{commands::DeleteMonitor, use_cases::DeleteMonitorUseCase};
use domain::shared::ids::UserId;

use crate::{error::ApiError, state::AppState, validation::parse_monitor_id};

/// Deletes a monitor through the API.
pub async fn delete_monitor(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("API: delete_monitor");

    let id = parse_monitor_id(&id)?;

    let use_case = DeleteMonitorUseCase::new(state.monitor_repository.clone());

    let deleted = use_case
        .execute(DeleteMonitor { id, user_id })
        .await
        .map_err(|err| ApiError::internal(format!("failed to delete monitor: {err}")))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("monitor not found"))
    }
}
