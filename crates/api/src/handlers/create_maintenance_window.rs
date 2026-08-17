use std::sync::Arc;

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use tracing::info;

use application::{
    commands::CreateMaintenanceWindow,
    use_cases::{CreateMaintenanceWindowError, CreateMaintenanceWindowUseCase},
};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError, handlers::feature_types::CreateMaintenanceWindowRequest, state::AppState,
    validation::parse_monitor_id,
};

/// Creates a maintenance window for a monitor.
///
/// `POST /api/v1/monitors/{monitor_id}/maintenance-windows`
///
/// Requires the monitor to be owned by the caller — enforced by
/// `MonitorRepository::find_by_id_for_user` inside the use case.
pub async fn create_maintenance_window(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(monitor_id): Path<String>,
    Json(payload): Json<CreateMaintenanceWindowRequest>,
) -> Result<(StatusCode, Json<domain::MaintenanceWindow>), ApiError> {
    info!("API: create_maintenance_window");

    let monitor_id = parse_monitor_id(&monitor_id)?;

    let use_case = CreateMaintenanceWindowUseCase::new(
        state.monitor_repository.clone(),
        state.maintenance_window_repository.clone(),
        Arc::new(state.id_generator.clone()),
    );

    let window = use_case
        .execute(CreateMaintenanceWindow {
            monitor_id,
            user_id,
            starts_at: payload.starts_at,
            ends_at: payload.ends_at,
            reason: payload.reason,
        })
        .await
        .map_err(|err| match err {
            CreateMaintenanceWindowError::MonitorNotFound => {
                ApiError::not_found("monitor not found")
            }
            CreateMaintenanceWindowError::InvalidWindow(msg) => ApiError::bad_request(msg),
            CreateMaintenanceWindowError::Other(err) => {
                ApiError::internal_with_source("failed to create maintenance window", err)
            }
        })?;

    Ok((StatusCode::CREATED, Json(window)))
}
