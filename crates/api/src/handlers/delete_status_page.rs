use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tracing::info;

use application::use_cases::DeleteStatusPageUseCase;
use domain::shared::ids::UserId;

use crate::{error::ApiError, state::AppState, validation::parse_status_page_id};

/// Deletes a status page through the API.
pub async fn delete_status_page(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("API: delete_status_page");

    let id = parse_status_page_id(&id)?;

    let use_case = DeleteStatusPageUseCase::new(state.status_page_repository.clone());

    let deleted = use_case
        .execute(id, user_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to delete status page", err))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("status page not found"))
    }
}
