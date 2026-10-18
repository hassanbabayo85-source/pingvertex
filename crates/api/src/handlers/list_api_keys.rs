use axum::{
    Json,
    extract::{Extension, Query, State},
    http::{HeaderMap, StatusCode},
};
use tracing::info;

use application::use_cases::ListApiKeysUseCase;
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::{
        api_key_types::ApiKeySummaryResponse,
        pagination::{PaginationQuery, pagination_headers},
    },
    state::AppState,
};

/// Lists every API key belonging to the caller's tenant, cursor-paginated.
///
/// `GET /api/v1/api-keys?limit=50&cursor=<opaque>`
///
/// Response body is a JSON array (unchanged). When more
/// pages exist, an `X-Next-Cursor` response header carries the opaque
/// cursor. Absence of the header = "last page".
pub async fn list_api_keys(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Query(query): Query<PaginationQuery>,
) -> Result<(StatusCode, HeaderMap, Json<Vec<ApiKeySummaryResponse>>), ApiError> {
    info!("API: list_api_keys");

    let use_case = ListApiKeysUseCase::new(state.api_key_repository.clone());

    let page = use_case
        .execute_paginated(authenticated_user, query.into_page_request())
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list api keys", err))?;

    let headers = pagination_headers(page.next_cursor)?;
    let body: Vec<ApiKeySummaryResponse> = page
        .items
        .into_iter()
        .map(ApiKeySummaryResponse::from)
        .collect();

    Ok((StatusCode::OK, headers, Json(body)))
}
