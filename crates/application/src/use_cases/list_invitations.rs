use chrono::Utc;
use domain::{
    TenantInvitation,
    shared::types::{AuthenticatedUser, Permission},
};

use crate::ports::TenantInvitationRepository;
use crate::use_cases::invitation_common::InvitationError;

/// Lists pending invitations for the caller's current tenant.
///
/// `ManageUsers` permission is required, matching member management.
pub struct ListInvitationsUseCase<IR> {
    invitation_repository: IR,
}

impl<IR> ListInvitationsUseCase<IR>
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
        actor: AuthenticatedUser,
    ) -> Result<Vec<TenantInvitation>, InvitationError> {
        if !actor.can(Permission::ManageUsers) {
            return Err(InvitationError::Forbidden);
        }

        let now = Utc::now();
        let invitations = self
            .invitation_repository
            .list_by_tenant(actor.tenant_context().tenant_id())
            .await
            .map_err(InvitationError::Internal)?;

        Ok(invitations
            .into_iter()
            .filter(|invitation| invitation.is_pending(now))
            .collect())
    }
}
