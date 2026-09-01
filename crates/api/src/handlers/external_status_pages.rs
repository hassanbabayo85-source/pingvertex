use axum::{
    Json,
    extract::{Extension, Path, State},
};
use tracing::info;

use application::{
    commands::UpdateStatusPage,
    use_cases::{ListStatusPagesUseCase, UpdateStatusPageError, UpdateStatusPageUseCase},
};

use crate::{
    error::ApiError,
    handlers::types::{StatusPageResponse, StatusPageUpdateRequest},
    middleware::api_key_auth::ApiKeyPrincipal,
    state::AppState,
    validation::{parse_monitor_id, parse_status_page_id},
};

/// Lists status pages belonging to the API key's tenant.
///
/// `GET /api/v1/external/status-pages`
///
/// Gated on `ApiKeyScope::StatusPageRead`.
pub async fn list_status_pages_via_api_key(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
) -> Result<Json<Vec<StatusPageResponse>>, ApiError> {
    info!("API: list_status_pages_via_api_key");

    let use_case = ListStatusPagesUseCase::new(state.status_page_repository.clone());

    let status_pages = use_case
        .execute_for_tenant(principal.tenant_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list status pages", err))?;

    Ok(Json(
        status_pages
            .into_iter()
            .map(StatusPageResponse::from)
            .collect(),
    ))
}

/// Updates a status page belonging to the API key's tenant.
///
/// `PUT /api/v1/external/status-pages/{id}`
///
/// Gated on `ApiKeyScope::StatusPageWrite`.
pub async fn update_status_page_via_api_key(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    Path(id): Path<String>,
    Json(payload): Json<StatusPageUpdateRequest>,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: update_status_page_via_api_key");

    let status_page_id = parse_status_page_id(&id)?;
    let monitor_ids = payload
        .monitor_ids
        .iter()
        .map(|id| parse_monitor_id(id))
        .collect::<Result<Vec<_>, _>>()?;

    let command = UpdateStatusPage {
        id: status_page_id,
        name: payload.name,
        monitor_ids,
        logo_url: payload.logo_url,
        primary_color: payload.primary_color,
    };

    let use_case = UpdateStatusPageUseCase::new(
        state.status_page_repository.clone(),
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let status_page = use_case
        .execute_for_tenant(command, principal.tenant_id, principal.user_id)
        .await
        .map_err(|err| match err {
            // `execute_for_tenant` never constructs `Forbidden` — that
            // check only runs in `execute`'s `AuthenticatedUser` wrapper,
            // which this API-key-authenticated path doesn't call. Kept
            // here only because the match must be exhaustive.
            UpdateStatusPageError::Forbidden => {
                ApiError::internal("unexpected authorization error")
            }
            UpdateStatusPageError::MonitorNotFound { .. } => {
                ApiError::not_found("monitor not found")
            }
            UpdateStatusPageError::Other(inner) => {
                ApiError::internal_with_source("failed to update status page", inner)
            }
        })?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    Ok(Json(StatusPageResponse::from(status_page)))
}
