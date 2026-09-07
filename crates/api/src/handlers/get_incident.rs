use axum::{
    Json,
    extract::{Extension, Path, State},
};
use domain::shared::{
    policies::{AuthorizationPolicy, TenantOwned},
    types::AuthenticatedUser,
};
use tracing::info;

use crate::{
    error::ApiError, handlers::IncidentResponse, state::AppState, validation::parse_incident_id,
};

/// Gets a single incident through the authenticated API.
///
/// The incident itself is not tenant-scoped in the repository API, so this
/// handler performs an explicit incident -> monitor -> tenant authorization
/// check before returning any incident data. This prevents an authenticated
/// user from probing another tenant's incident UUID.
pub async fn get_incident(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(incident_id): Path<String>,
) -> Result<Json<IncidentResponse>, ApiError> {
    info!("API: get_incident");

    let incident_id = parse_incident_id(&incident_id)?;
    let incident = state
        .incident_repository
        .find_by_id(incident_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch incident", err))?
        .ok_or_else(|| ApiError::not_found("incident not found"))?;

    let monitor = state
        .monitor_repository
        .find_by_id(incident.monitor_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to authorize incident", err))?
        .ok_or_else(|| ApiError::not_found("incident not found"))?;

    if !AuthorizationPolicy::same_tenant(&authenticated_user.tenant_context(), monitor.tenant_id())
    {
        return Err(ApiError::not_found("incident not found"));
    }

    Ok(Json(IncidentResponse::from(incident)))
}
