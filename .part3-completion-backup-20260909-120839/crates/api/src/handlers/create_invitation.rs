use axum::{Json, extract::Extension, extract::State, http::StatusCode};
use domain::shared::types::AuthenticatedUser;
use tracing::info;

use application::use_cases::{CreateInvitationCommand, CreateInvitationUseCase};

use crate::{
    auth_state::AuthState,
    handlers::invitation_types::{
        CreateInvitationRequest, CreatedInvitationResponse, map_invitation_error,
    },
    validation::parse_role,
};

/// Creates a tenant invitation.
///
/// `POST /api/v1/invitations`
///
/// The raw `invitation_token` is returned exactly once; the client must
/// pass it to the invitee out-of-band (or email it later when a mailer
/// worker is available).
pub async fn create_invitation(
    State(auth_state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateInvitationRequest>,
) -> Result<(StatusCode, Json<CreatedInvitationResponse>), crate::error::ApiError> {
    info!("API: create_invitation");

    let role = parse_role(&payload.role)?;

    let use_case = CreateInvitationUseCase::new(
        auth_state.id_generator.clone(),
        auth_state.password_hasher.clone(),
        auth_state.invitation_repository.clone(),
    );

    let created = use_case
        .execute(
            CreateInvitationCommand {
                email: payload.email,
                role,
            },
            authenticated_user,
        )
        .await
        .map_err(map_invitation_error)?;

    let response = CreatedInvitationResponse {
        id: created.invitation.id.into_uuid(),
        email: created.invitation.email,
        role: created.invitation.role,
        expires_at: created.invitation.expires_at,
        created_at: created.invitation.created_at,
        invitation_token: created.raw_token,
    };

    Ok((StatusCode::CREATED, Json(response)))
}
