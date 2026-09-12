use axum::{extract::Extension, extract::Path, extract::State, http::StatusCode};
use tracing::info;

use application::{commands::DeleteApiKey, use_cases::DeleteApiKeyUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, state::AppState, validation::parse_api_key_id};

/// Revokes (deletes) an API key.
///
/// `DELETE /api/v1/api-keys/{id}`
///
/// Tenant-wide: any member of the caller's tenant may revoke any key
/// belonging to that tenant — see `DeleteApiKeyUseCase`.
pub async fn delete_api_key(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("API: delete_api_key");

    let id = parse_api_key_id(&id)?;

    let use_case = DeleteApiKeyUseCase::new(
        state.api_key_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let deleted = use_case
        .execute(DeleteApiKey { id }, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to delete api key", err))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        // Same response whether the key never existed or belongs to a
        // different tenant — see `DeleteApiKeyUseCase`.
        Err(ApiError::not_found("api key not found"))
    }
}
