use axum::{Json, extract::Extension, extract::State, http::StatusCode};
use domain::shared::types::AuthenticatedUser;
use tracing::info;

use application::use_cases::ListInvitationsUseCase;

use crate::{
    auth_state::AuthState,
    handlers::invitation_types::{InvitationResponse, map_invitation_error},
};

/// Lists pending invitations for the caller's current tenant.
///
/// `GET /api/v1/invitations/pending`
pub async fn list_invitations(
    State(auth_state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<(StatusCode, Json<Vec<InvitationResponse>>), crate::error::ApiError> {
    info!("API: list_invitations");

    let use_case = ListInvitationsUseCase::new(auth_state.invitation_repository.clone());

    let invitations = use_case
        .execute(authenticated_user)
        .await
        .map_err(map_invitation_error)?;

    let response: Vec<InvitationResponse> =
        invitations.iter().map(InvitationResponse::from).collect();

    Ok((StatusCode::OK, Json(response)))
}
