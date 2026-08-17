use axum::{
    extract::{Path, State},
    response::Response,
};
use tracing::info;

use crate::{
    error::ApiError, handlers::public_status_page::render_public_status_page, state::AppState,
};

/// Returns the public, unauthenticated status page for `subdomain`
/// (`acme.pingvertex.com` -> `subdomain = "acme"`).
pub async fn get_public_status_page(
    State(state): State<AppState>,
    Path(subdomain): Path<String>,
) -> Result<Response, ApiError> {
    info!("API: get_public_status_page");

    let status_page = state
        .status_page_repository
        .find_by_subdomain(&subdomain)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch status page", err))?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    render_public_status_page(&state, status_page).await
}
