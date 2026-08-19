use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tracing::info;

use application::{
    commands::AcknowledgeIncident,
    use_cases::{AcknowledgeIncidentError, AcknowledgeIncidentUseCase},
};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, state::AppState, validation::parse_incident_id};

/// Acknowledges an incident.
///
/// `POST /api/v1/incidents/{incident_id}/acknowledge`
///
/// The caller must own the monitor associated with the incident.
pub async fn acknowledge_incident(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(incident_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("API: acknowledge_incident");

    let incident_id = parse_incident_id(&incident_id)?;

    let use_case = AcknowledgeIncidentUseCase::new(
        state.monitor_repository.clone(),
        state.incident_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    use_case
        .execute(AcknowledgeIncident { incident_id }, authenticated_user)
        .await
        .map_err(|err| match err {
            AcknowledgeIncidentError::NotFound => ApiError::not_found("incident not found"),
            AcknowledgeIncidentError::AlreadyResolved => {
                ApiError::bad_request("incident is already resolved")
            }
            AcknowledgeIncidentError::Other(err) => {
                ApiError::internal_with_source("failed to acknowledge incident", err)
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}
