use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use domain::Incident;
use serde::Serialize;
use tracing::info;

use application::use_cases::{ListIncidentsError, ListIncidentsUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, state::AppState, validation::parse_monitor_id};

/// Full incident response exposed to the authenticated API.
///
/// Includes the tracking fields:
/// - `started_at` as `first_seen`
/// - `last_seen_at`
/// - `duration_ms` (open: started_at → now, resolved: started_at → resolved_at)
#[derive(Debug, Clone, Serialize)]
pub struct IncidentResponse {
    pub id: String,
    pub monitor_id: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub duration_ms: i64,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<String>,
    pub occurred_during_maintenance: bool,
    pub escalation_step_index: u32,
    pub last_escalation_at: Option<DateTime<Utc>>,
}

impl From<Incident> for IncidentResponse {
    fn from(incident: Incident) -> Self {
        let duration_ms = incident.duration(Utc::now()).num_milliseconds();

        Self {
            id: incident.id.0.to_string(),
            monitor_id: incident.monitor_id.0.to_string(),
            first_seen: incident.started_at,
            last_seen_at: incident.last_seen_at,
            resolved_at: incident.resolved_at,
            duration_ms,
            acknowledged_at: incident.acknowledged_at,
            acknowledged_by: incident.acknowledged_by.map(|id| id.0.to_string()),
            occurred_during_maintenance: incident.occurred_during_maintenance,
            escalation_step_index: incident.escalation_step_index,
            last_escalation_at: incident.last_escalation_at,
        }
    }
}

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
        Ok(incidents) => {
            let responses: Vec<IncidentResponse> =
                incidents.into_iter().map(IncidentResponse::from).collect();
            (StatusCode::OK, Json(responses)).into_response()
        }
        Err(ListIncidentsError::NotFound) => {
            ApiError::not_found("monitor not found").into_response()
        }
        Err(ListIncidentsError::Other(err)) => {
            ApiError::internal_with_source("failed to list incidents", err).into_response()
        }
    }
}
