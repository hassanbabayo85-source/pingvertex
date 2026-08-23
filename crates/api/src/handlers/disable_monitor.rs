use axum::{
    Json,
    extract::{Extension, Path, State},
};
use tracing::info;

use application::use_cases::{SetMonitorEnabledCommand, SetMonitorEnabledUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError, handlers::types::MonitorResponse, state::AppState,
    validation::parse_monitor_id,
};

pub async fn disable_monitor(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<MonitorResponse>, ApiError> {
    info!("API: disable_monitor");

    let monitor_id = parse_monitor_id(&id)?;

    let use_case = SetMonitorEnabledUseCase::new(
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let monitor = use_case
        .execute(
            SetMonitorEnabledCommand {
                id: monitor_id,
                enabled: false,
            },
            authenticated_user,
        )
        .await
        .map_err(|err| ApiError::internal_with_source("failed to disable monitor", err))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    Ok(Json(MonitorResponse::from(monitor)))
}
