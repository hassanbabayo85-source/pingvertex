use axum::{
    extract::{Extension, State},
    Json,
};
use tracing::info;

use application::{queries::ListMonitors, use_cases::ListMonitorsUseCase};
use domain::shared::ids::UserId;

use crate::{error::ApiError, handlers::types::MonitorResponse, state::AppState};

/// Lists all monitors through the API.
pub async fn list_monitors(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> Result<Json<Vec<MonitorResponse>>, ApiError> {
    info!("API: list_monitors");

    let use_case = ListMonitorsUseCase::new(state.monitor_repository.clone());

    let monitors = use_case
        .execute(ListMonitors { user_id })
        .await
        .map_err(|err| ApiError::internal(format!("failed to list monitors: {err}")))?;

    Ok(Json(
        monitors.into_iter().map(MonitorResponse::from).collect(),
    ))
}
