use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
};
use tracing::info;

use application::{
    commands::UpdateStatusPage,
    pagination::PageRequest,
    use_cases::{ListStatusPagesUseCase, UpdateStatusPageError, UpdateStatusPageUseCase},
};

use crate::{
    error::ApiError,
    handlers::{
        list_monitors::{ListMonitorsQuery, NEXT_CURSOR_HEADER},
        types::{StatusPageResponse, StatusPageUpdateRequest},
    },
    middleware::api_key_auth::ApiKeyPrincipal,
    state::AppState,
    validation::{parse_monitor_id, parse_status_page_id},
};

/// Lists status pages belonging to the API key's tenant.
///
/// `GET /api/v1/external/status-pages?limit=50&cursor=<opaque>`
///
/// Gated on `ApiKeyScope::StatusPageRead`.
///
/// Converted to cursor-based pagination. Both `limit` and `cursor`
/// are optional. Limits are clamped to `MAX_PAGE_SIZE` (100).
pub async fn list_status_pages_via_api_key(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    Query(query): Query<ListMonitorsQuery>,
) -> Result<(StatusCode, HeaderMap, Json<Vec<StatusPageResponse>>), ApiError> {
    info!("API: list_status_pages_via_api_key");

    let page_request = PageRequest::new(query.limit, query.cursor.as_deref());
    let use_case = ListStatusPagesUseCase::new(state.status_page_repository.clone());

    let page = use_case
        .execute_paginated_for_tenant(principal.tenant_id, page_request)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list status pages", err))?;

    let response: Vec<StatusPageResponse> = page
        .items
        .into_iter()
        .map(StatusPageResponse::from)
        .collect();

    let mut headers = HeaderMap::new();
    if let Some(cursor) = page.next_cursor {
        let value = HeaderValue::from_str(&cursor)
            .map_err(|_| ApiError::internal("failed to encode next cursor"))?;
        headers.insert(NEXT_CURSOR_HEADER, value);
    }

    Ok((StatusCode::OK, headers, Json(response)))
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
