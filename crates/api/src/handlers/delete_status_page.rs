use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tracing::info;

use application::use_cases::DeleteStatusPageUseCase;
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, state::AppState, validation::parse_status_page_id};

/// Deletes a status page through the API.
pub async fn delete_status_page(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("API: delete_status_page");

    let id = parse_status_page_id(&id)?;

    let use_case = DeleteStatusPageUseCase::new(
        state.status_page_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let deleted = use_case
        .execute(id, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to delete status page", err))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("status page not found"))
    }
}
