use axum::{
    Json,
    extract::{Extension, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
};
use tracing::info;

use crate::{
    error::ApiError,
    handlers::list_monitors::{ListMonitorsQuery, NEXT_CURSOR_HEADER},
    handlers::types::MonitorResponse,
    middleware::api_key_auth::ApiKeyPrincipal,
    state::AppState,
};
use application::{
    pagination::PageRequest, use_cases::list_external_monitors::ListExternalMonitorsUseCase,
};

/// Lists monitors for the tenant that owns the API key used to
/// authenticate this request.
///
/// `GET /api/v1/external/monitors?limit=50&cursor=<opaque>`
///
/// Authenticated via `ApiKeyAuthMiddleware` + `require_api_key_scope`
/// (wired in `router.rs`) instead of `jwt_auth` — this is the reference
/// endpoint demonstrating end-to-end `X-API-Key` authentication with
/// `ApiKeyScope::MonitorRead` enforcement.
///
/// Converted to cursor-based pagination. Both `limit` and `cursor`
/// are optional. Limits are clamped to `MAX_PAGE_SIZE` (100). Malformed cursors
/// are safely ignored and start from the first page.
pub async fn list_monitors_via_api_key(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    Query(query): Query<ListMonitorsQuery>,
) -> Result<(StatusCode, HeaderMap, Json<Vec<MonitorResponse>>), ApiError> {
    info!("API: list_monitors_via_api_key");

    let page_request = PageRequest::new(query.limit, query.cursor.as_deref());

    let use_case = ListExternalMonitorsUseCase::new(state.monitor_repository.clone());

    let page = use_case
        .execute(page_request, principal.tenant_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list external monitors", err))?;

    let response: Vec<MonitorResponse> = page
        .items
        .into_iter()
        .map(|mut monitor| {
            // External/List responses do not need monitor assertions. Clear them
            // explicitly so authenticated list callers cannot receive assertion payloads.
            monitor.assertions.clear();
            MonitorResponse::from(monitor)
        })
        .collect();

    let mut headers = HeaderMap::new();
    if let Some(cursor) = page.next_cursor {
        let value = HeaderValue::from_str(&cursor)
            .map_err(|_| ApiError::internal("failed to encode next cursor"))?;
        headers.insert(NEXT_CURSOR_HEADER, value);
    }

    Ok((StatusCode::OK, headers, Json(response)))
}
