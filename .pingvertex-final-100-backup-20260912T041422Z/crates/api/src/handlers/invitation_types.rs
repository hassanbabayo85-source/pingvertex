use chrono::{DateTime, Utc};
use domain::TenantInvitation;
use domain::shared::types::Role;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use application::use_cases::InvitationError;

use crate::error::ApiError;

#[derive(Debug, Deserialize)]
pub struct CreateInvitationRequest {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct AcceptInvitationRequest {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct InvitationResponse {
    pub id: Uuid,
    pub email: String,
    pub role: Role,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl From<TenantInvitation> for InvitationResponse {
    fn from(invitation: TenantInvitation) -> Self {
        Self {
            id: invitation.id.into_uuid(),
            email: invitation.email,
            role: invitation.role,
            expires_at: invitation.expires_at,
            created_at: invitation.created_at,
        }
    }
}

impl From<&TenantInvitation> for InvitationResponse {
    fn from(invitation: &TenantInvitation) -> Self {
        Self {
            id: invitation.id.into_uuid(),
            email: invitation.email.clone(),
            role: invitation.role,
            expires_at: invitation.expires_at,
            created_at: invitation.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CreatedInvitationResponse {
    pub id: Uuid,
    pub email: String,
    pub role: Role,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    /// Raw invitation token — returned exactly once at creation time.
    pub invitation_token: String,
}

pub fn map_invitation_error(err: InvitationError) -> ApiError {
    match err {
        InvitationError::Forbidden => ApiError::forbidden("forbidden"),
        InvitationError::NotFound | InvitationError::InvalidToken => {
            ApiError::not_found("invitation not found")
        }
        InvitationError::Conflict => ApiError::conflict("already a member"),
        InvitationError::InvalidOperation(msg) => ApiError::bad_request(msg),
        InvitationError::Internal(source) => {
            ApiError::internal_with_source("failed to process invitation", source)
        }
    }
}
