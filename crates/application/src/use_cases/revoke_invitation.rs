use chrono::Utc;
use domain::{
    InvitationId,
    shared::types::{AuthenticatedUser, Permission, Role},
};

use crate::ports::TenantInvitationRepository;
use crate::use_cases::invitation_common::InvitationError;

/// Revokes a pending invitation.
///
/// Authorization:
/// - Requires `Permission::ManageUsers`.
/// - The invitation must belong to the caller's tenant.
/// - Only `Owner` may revoke an invitation for `Owner` or `Admin`.
pub struct RevokeInvitationUseCase<IR> {
    invitation_repository: IR,
}

impl<IR> RevokeInvitationUseCase<IR>
where
    IR: TenantInvitationRepository,
{
    pub fn new(invitation_repository: IR) -> Self {
        Self {
            invitation_repository,
        }
    }

    pub async fn execute(
        &self,
        invitation_id: InvitationId,
        actor: AuthenticatedUser,
    ) -> Result<(), InvitationError> {
        if !actor.can(Permission::ManageUsers) {
            return Err(InvitationError::Forbidden);
        }

        let invitation = self
            .invitation_repository
            .find_by_id(invitation_id)
            .await
            .map_err(InvitationError::Internal)?
            .ok_or(InvitationError::NotFound)?;

        if invitation.tenant_id != actor.tenant_context().tenant_id() {
            return Err(InvitationError::Forbidden);
        }

        match invitation.role {
            Role::Owner | Role::Admin => {
                if actor.role() != Role::Owner {
                    return Err(InvitationError::Forbidden);
                }
            }
            Role::Operator | Role::Viewer | Role::Auditor => {}
        }

        if !invitation.is_pending(Utc::now()) {
            return Err(InvitationError::NotFound);
        }

        self.invitation_repository
            .mark_revoked(invitation.id, Utc::now())
            .await
            .map_err(InvitationError::Internal)?;

        Ok(())
    }
}
