use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tracing::info;

use application::{commands::DeleteMonitor, use_cases::DeleteMonitorUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, state::AppState, validation::parse_monitor_id};

/// Deletes a monitor through the API.
pub async fn delete_monitor(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("API: delete_monitor");

    let id = parse_monitor_id(&id)?;

    let use_case = DeleteMonitorUseCase::new(
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let deleted = use_case
        .execute(DeleteMonitor { id }, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to delete monitor", err))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("monitor not found"))
    }
}
