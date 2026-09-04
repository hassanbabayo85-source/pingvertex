use axum::{
    Json,
    extract::{Extension, State},
};
use tracing::info;

use application::{queries::ListMonitors, use_cases::ListMonitorsUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, handlers::types::MonitorResponse, state::AppState};

/// Lists all monitors through the API.
pub async fn list_monitors(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<MonitorResponse>>, ApiError> {
    info!("API: list_monitors");

    let use_case = ListMonitorsUseCase::new(state.monitor_repository.clone());

    let monitors = use_case
        .execute(ListMonitors, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list monitors", err))?;

    // List responses do not need monitor assertions. Clear them explicitly so
    // authenticated list callers cannot receive assertion payloads they did not
    // request, while GET /monitors/{id} can still round-trip them for editing.
    Ok(Json(
        monitors
            .into_iter()
            .map(|monitor| {
                let mut response = MonitorResponse::from(monitor);
                response.assertions.clear();
                response
            })
            .collect(),
    ))
}
