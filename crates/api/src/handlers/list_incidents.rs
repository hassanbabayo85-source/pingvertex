use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::info;

use application::use_cases::{ListIncidentsError, ListIncidentsUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, state::AppState, validation::parse_monitor_id};

/// Lists all incidents for a monitor (any user in the monitor's tenant).
pub async fn list_incidents(
    Path(id): Path<String>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_incidents");

    let monitor_id = match parse_monitor_id(&id) {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let use_case = ListIncidentsUseCase::new(
        state.monitor_repository.clone(),
        state.incident_repository.clone(),
    );

    match use_case.execute(monitor_id, authenticated_user).await {
        Ok(incidents) => (StatusCode::OK, Json(incidents)).into_response(),
        Err(ListIncidentsError::NotFound) => {
            ApiError::not_found("monitor not found").into_response()
        }
        Err(ListIncidentsError::Other(err)) => {
            ApiError::internal_with_source("failed to list incidents", err).into_response()
        }
    }
}
