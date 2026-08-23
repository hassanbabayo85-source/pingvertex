use axum::{Json, extract::Extension, extract::Path, extract::State, http::StatusCode};
use domain::InvitationId;
use domain::shared::types::AuthenticatedUser;
use tracing::info;
use uuid::Uuid;

use application::use_cases::RevokeInvitationUseCase;

use crate::{auth_state::AuthState, handlers::invitation_types::map_invitation_error};

/// Revokes a pending invitation.
///
/// `DELETE /api/v1/invitations/{id}`
pub async fn revoke_invitation(
    State(auth_state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(invitation_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), crate::error::ApiError> {
    info!("API: revoke_invitation");

    let use_case = RevokeInvitationUseCase::new(auth_state.invitation_repository.clone());

    use_case
        .execute(InvitationId::from_uuid(invitation_id), authenticated_user)
        .await
        .map_err(map_invitation_error)?;

    Ok((StatusCode::NO_CONTENT, Json(serde_json::json!({}))))
}
