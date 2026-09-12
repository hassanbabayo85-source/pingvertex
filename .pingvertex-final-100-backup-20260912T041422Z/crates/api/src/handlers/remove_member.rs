use axum::{Json, extract::Extension, extract::Path, extract::State, http::StatusCode};
use domain::shared::ids::UserId;
use domain::shared::types::AuthenticatedUser;
use tracing::info;
use uuid::Uuid;

use application::use_cases::RemoveMemberUseCase;

use crate::{auth_state::AuthState, error::ApiError};

/// Removes a member from the caller's tenant.
///
/// `DELETE /api/v1/members/{user_id}`
pub async fn remove_member(
    State(auth_state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(user_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    info!("API: remove_member");

    let use_case = RemoveMemberUseCase::new(auth_state.tenant_membership_repository.clone());

    use_case
        .with_audit_log_repository(auth_state.audit_log_entry_repository.clone())
        .execute(UserId(user_id), authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to remove member", err))?;

    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!({}))))
}
