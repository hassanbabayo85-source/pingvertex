use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use tracing::info;

use application::{
    commands::AcknowledgeIncident,
    use_cases::{
        AcknowledgeIncidentError, AcknowledgeIncidentUseCase, ListIncidentsError,
        ListIncidentsUseCase,
    },
};

use crate::{
    error::ApiError,
    handlers::IncidentResponse,
    middleware::api_key_auth::ApiKeyPrincipal,
    state::AppState,
    validation::{parse_incident_id, parse_monitor_id},
};

/// Lists incidents for a monitor belonging to the API key's tenant.
///
/// `GET /api/v1/external/incidents/{monitor_id}`
///
/// Gated on `ApiKeyScope::IncidentRead`.
pub async fn list_incidents_via_api_key(
    Path(id): Path<String>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_incidents_via_api_key");

    let monitor_id = match parse_monitor_id(&id) {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let use_case = ListIncidentsUseCase::new(
        state.monitor_repository.clone(),
        state.incident_repository.clone(),
    );

    match use_case
        .execute_for_tenant(monitor_id, principal.tenant_id)
        .await
    {
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

/// Acknowledges an incident belonging to the API key's tenant.
///
/// `POST /api/v1/external/incidents/{incident_id}/acknowledge`
///
/// Gated on `ApiKeyScope::IncidentWrite`. The audit log records the
/// key's `user_id` (its creator, per `ApiKeyPrincipal`) as the acting
/// user — an audit trail, not an access-control gate, matching how
/// `ApiKey::user_id` is treated everywhere else in this codebase.
pub async fn acknowledge_incident_via_api_key(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    Path(incident_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("API: acknowledge_incident_via_api_key");

    let incident_id = parse_incident_id(&incident_id)?;

    let use_case = AcknowledgeIncidentUseCase::new(
        state.monitor_repository.clone(),
        state.incident_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    use_case
        .execute_for_tenant(
            AcknowledgeIncident { incident_id },
            principal.tenant_id,
            principal.user_id,
        )
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
