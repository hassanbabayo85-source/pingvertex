use axum::{
    Json,
    extract::{Extension, Query, State},
    http::HeaderMap,
};
use tracing::info;

use application::use_cases::ListStatusPagesUseCase;
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::{
        pagination::{PaginationQuery, pagination_headers},
        types::StatusPageResponse,
    },
    state::AppState,
};

/// Lists status pages owned by the caller's tenant, cursor-paginated.
///
/// `GET /api/v1/status-pages?limit=50&cursor=<opaque>`
///
/// Response body is a JSON array (unchanged from before PART 8, so
/// existing clients keep working). When more pages exist, an
/// `X-Next-Cursor` response header carries the opaque cursor to pass
/// back via `?cursor=`. Absence of the header = "last page".
pub async fn list_status_pages(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Query(query): Query<PaginationQuery>,
) -> Result<(HeaderMap, Json<Vec<StatusPageResponse>>), ApiError> {
    info!("API: list_status_pages");

    let use_case = ListStatusPagesUseCase::new(state.status_page_repository.clone());

    let page = use_case
        .execute_paginated(authenticated_user, query.into_page_request())
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list status pages", err))?;

    let headers = pagination_headers(page.next_cursor)?;
    let body: Vec<StatusPageResponse> = page
        .items
        .into_iter()
        .map(StatusPageResponse::from)
        .collect();

    Ok((headers, Json(body)))
}
