use axum::{Json, extract::Extension, extract::State, http::StatusCode};
use domain::shared::types::AuthenticatedUser;
use tracing::info;

use application::use_cases::AcceptInvitationUseCase;

use crate::{
    auth_state::AuthState,
    handlers::{
        invitation_types::{AcceptInvitationRequest, map_invitation_error},
        members_types::TenantMembershipResponse,
    },
};

/// Redeems an invitation token and creates a membership.
///
/// `POST /api/v1/invitations/accept`
pub async fn accept_invitation(
    State(auth_state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<AcceptInvitationRequest>,
) -> Result<(StatusCode, Json<TenantMembershipResponse>), crate::error::ApiError> {
    info!("API: accept_invitation");

    let use_case = AcceptInvitationUseCase::new(
        auth_state.invitation_repository.clone(),
        auth_state.tenant_membership_repository.clone(),
        auth_state.user_repository.clone(),
        auth_state.password_hasher.clone(),
    );

    let membership = use_case
        .execute(payload.token, authenticated_user)
        .await
        .map_err(map_invitation_error)?;

    Ok((
        StatusCode::CREATED,
        Json(TenantMembershipResponse::from(membership)),
    ))
}
