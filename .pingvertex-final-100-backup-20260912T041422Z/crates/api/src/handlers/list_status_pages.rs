use axum::{
    Json,
    extract::{Extension, State},
};
use tracing::info;

use application::use_cases::ListStatusPagesUseCase;
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, handlers::types::StatusPageResponse, state::AppState};

/// Lists all status pages owned by the caller.
pub async fn list_status_pages(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<StatusPageResponse>>, ApiError> {
    info!("API: list_status_pages");

    let use_case = ListStatusPagesUseCase::new(state.status_page_repository.clone());

    let status_pages = use_case
        .execute(authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list status pages", err))?;

    Ok(Json(
        status_pages
            .into_iter()
            .map(StatusPageResponse::from)
            .collect(),
    ))
}
