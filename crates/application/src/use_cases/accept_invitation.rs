use chrono::Utc;
use domain::{TenantMembership, shared::types::AuthenticatedUser};

use crate::ports::{
    PasswordHasher, TenantInvitationRepository, TenantMembershipRepository, UserRepository,
};
use crate::use_cases::{
    invitation_common::InvitationError, invitation_support::invitation_token_prefix,
};

/// Redeems a raw invitation token and creates the corresponding
/// membership.
///
/// SECURITY:
/// - The token is looked up by prefix, then the full raw token is
///   re-verified against the stored Argon2id hash.
/// - The authenticated user's email MUST match the invitation email.
/// - The granted role is exactly the invited role — never more.
/// - The invitation is single-use: `mark_accepted` only succeeds once.
pub struct AcceptInvitationUseCase<IR, MR, UR, H> {
    invitation_repository: IR,
    membership_repository: MR,
    user_repository: UR,
    password_hasher: H,
}

impl<IR, MR, UR, H> AcceptInvitationUseCase<IR, MR, UR, H>
where
    IR: TenantInvitationRepository,
    MR: TenantMembershipRepository,
    UR: UserRepository,
    H: PasswordHasher,
{
    pub fn new(
        invitation_repository: IR,
        membership_repository: MR,
        user_repository: UR,
        password_hasher: H,
    ) -> Self {
        Self {
            invitation_repository,
            membership_repository,
            user_repository,
            password_hasher,
        }
    }

    pub async fn execute(
        &self,
        raw_token: String,
        actor: AuthenticatedUser,
    ) -> Result<TenantMembership, InvitationError> {
        let prefix =
            invitation_token_prefix(&raw_token).map_err(|_| InvitationError::InvalidToken)?;

        let invitation = self
            .invitation_repository
            .find_by_prefix(&prefix)
            .await
            .map_err(InvitationError::Internal)?
            .ok_or(InvitationError::InvalidToken)?;

        if !invitation.is_pending(Utc::now()) {
            return Err(InvitationError::InvalidToken);
        }

        let valid = self
            .password_hasher
            .verify(&raw_token, &invitation.token_hash)
            .await
            .map_err(InvitationError::Internal)?;

        if !valid {
            return Err(InvitationError::InvalidToken);
        }

        let user = self
            .user_repository
            .find_by_id(&actor.user_id())
            .await
            .map_err(InvitationError::Internal)?
            .ok_or(InvitationError::InvalidToken)?;

        // Wrong recipient must not learn that this invitation exists.
        if user.email().as_str() != invitation.email {
            return Err(InvitationError::InvalidToken);
        }

        if self
            .membership_repository
            .find_by_tenant_and_user(invitation.tenant_id, actor.user_id())
            .await
            .map_err(InvitationError::Internal)?
            .is_some()
        {
            return Err(InvitationError::Conflict);
        }

        let now = Utc::now();

        // Single-use first, then persist the membership. If the insert
        // fails after this point, the invitation is consumed rather than
        // allowing duplicate redemption.
        self.invitation_repository
            .mark_accepted(invitation.id, now)
            .await
            .map_err(InvitationError::Internal)?;

        let membership = TenantMembership {
            tenant_id: invitation.tenant_id,
            user_id: actor.user_id(),
            role: invitation.role,
            invited_at: invitation.created_at,
            joined_at: Some(now),
        };

        self.membership_repository
            .create(&membership)
            .await
            .map_err(InvitationError::Internal)?;

        Ok(membership)
    }
}
