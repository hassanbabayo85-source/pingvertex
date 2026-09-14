use axum::{
    Json,
    extract::{Extension, Query, State},
    http::{HeaderMap, HeaderValue, header::HeaderName},
};
use serde::Deserialize;
use tracing::info;

use application::{pagination::PageRequest, use_cases::ListMonitorsUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, handlers::types::MonitorResponse, state::AppState};

/// PART 8 — query parameters for cursor-based pagination.
///
/// Both are optional: an empty request gets the first `DEFAULT_PAGE_SIZE`
/// monitors. `limit` is clamped into `[1, MAX_PAGE_SIZE]` by
/// `PageRequest::new`, so `?limit=1000000` cannot force an unbounded
/// query. `cursor` is opaque; a malformed value is silently ignored
/// (starts from the first page) rather than leaked back to the client as
/// a distinguishable error.
#[derive(Debug, Deserialize)]
pub struct ListMonitorsQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

/// Response header name carrying the opaque cursor for the next page.
/// Present ONLY when a next page exists; absent on the last page.
const NEXT_CURSOR_HEADER: HeaderName = HeaderName::from_static("x-next-cursor");

/// Lists monitors for the caller's tenant, cursor-paginated.
///
/// `GET /api/v1/monitors?limit=50&cursor=<opaque>`
///
/// Response body is a JSON array of monitors (unchanged from before
/// PART 8, so existing clients keep working). When more pages exist, an
/// `X-Next-Cursor` response header carries the opaque cursor to pass
/// back via `?cursor=`. Absence of the header means "last page".
pub async fn list_monitors(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Query(query): Query<ListMonitorsQuery>,
) -> Result<(HeaderMap, Json<Vec<MonitorResponse>>), ApiError> {
    info!("API: list_monitors");

    let page_request = PageRequest::new(query.limit, query.cursor.as_deref());

    let use_case = ListMonitorsUseCase::new(state.monitor_repository.clone());

    let page = use_case
        .execute_paginated(page_request, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list monitors", err))?;

    let billing_blocks = state
        .monitor_repository
        .billing_blocks_for_tenant(authenticated_user.tenant_context().tenant_id())
        .await
        .map_err(|err| {
            ApiError::internal_with_source("failed to load monitor billing state", err)
        })?;

    let body: Vec<MonitorResponse> = page
        .items
        .into_iter()
        .map(|monitor| {
            let mut response = MonitorResponse::from(monitor.clone());
            if let Some(reason) = billing_blocks.get(&monitor.id) {
                response.billing_blocked = true;
                response.billing_block_reason = Some(reason.clone());
            }
            // List responses do not need monitor assertions. Clear them
            // explicitly so authenticated list callers cannot receive
            // assertion payloads they did not request.
            response.assertions.clear();
            response
        })
        .collect();

    let mut headers = HeaderMap::new();
    if let Some(cursor) = page.next_cursor {
        let value = HeaderValue::from_str(&cursor)
            .map_err(|_| ApiError::internal("failed to encode next cursor"))?;
        headers.insert(NEXT_CURSOR_HEADER, value);
    }

    Ok((headers, Json(body)))
}
