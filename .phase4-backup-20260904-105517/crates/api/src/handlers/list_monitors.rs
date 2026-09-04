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

    Ok(Json(
        monitors.into_iter().map(MonitorResponse::from).collect(),
    ))
}
