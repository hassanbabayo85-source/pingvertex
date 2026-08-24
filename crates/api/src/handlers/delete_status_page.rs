use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tracing::{info, warn};

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

    let status_page_id = parse_status_page_id(&id)?;

    let use_case = DeleteStatusPageUseCase::new(
        state.status_page_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let deleted = use_case
        .execute(status_page_id, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to delete status page", err))?;

    if deleted {
        // B34: see the matching comment in `update_status_page` — this
        // was previously dead code (`StatusPageCache::invalidate` was
        // never called anywhere in the codebase). Deleting a status page
        // should stop serving its old cached response immediately rather
        // than leaving a ghost page reachable for up to the TTL.
        let cache_key = format!("status_page:id:{}", status_page_id.0);
        if let Err(err) = state.status_page_cache.invalidate(&cache_key).await {
            warn!(
                status_page_id = %status_page_id.0,
                error = %err,
                "failed to invalidate cached status page after delete"
            );
        }

        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("status page not found"))
    }
}
