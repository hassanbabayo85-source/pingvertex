use std::sync::Arc;

use chrono::Utc;
use domain::{
    TenantMembership,
    entities::MAX_TEAM_MEMBERS,
    shared::types::AuthenticatedUser,
};

use crate::errors::PlanLimitError;
use crate::ports::monitor_repository::QuotaCheckOutcome;
use crate::ports::{
    EntitlementResolver, PasswordHasher, TenantInvitationRepository, TenantMembershipRepository,
    UserRepository,
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
///
/// TASK 4A — `max_team_members` quota:
/// - When `entitlement_resolver` is wired, membership creation goes
///   through `TenantMembershipRepository::save_if_within_quota`, which
///   is atomic (`pg_advisory_xact_lock` + `COUNT` + `INSERT` inside one
///   transaction). Concurrent acceptances across different tenants
///   and different invitations are serialized per tenant.
/// - If the quota is exhausted, the acceptance is rejected with a
///   `PlanLimitError` and **the invitation remains pending** — the
///   invited user can retry after the admin removes a member.
/// - On success, `mark_accepted` is called **after** the membership
///   is persisted. If `mark_accepted` fails (rare race), the user is
///   still a member (the slot was taken) and retrying the same token
///   hits the `find_by_tenant_and_user` guard below with `Conflict`.
pub struct AcceptInvitationUseCase<IR, MR, UR, H> {
    invitation_repository: IR,
    membership_repository: MR,
    user_repository: UR,
    password_hasher: H,
    entitlement_resolver: Option<Arc<dyn EntitlementResolver>>,
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
            entitlement_resolver: None,
        }
    }

    /// TASK 4A — installs the authoritative `EntitlementResolver` and
    /// turns on atomic quota enforcement at acceptance time.
    pub fn with_entitlement_resolver(
        mut self,
        resolver: Arc<dyn EntitlementResolver>,
    ) -> Self {
        self.entitlement_resolver = Some(resolver);
        self
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

        let membership = TenantMembership {
            tenant_id: invitation.tenant_id,
            user_id: actor.user_id(),
            role: invitation.role,
            invited_at: invitation.created_at,
            joined_at: Some(now),
        };

        // ── TASK 4A: atomic quota check + insert (if wired) ─────
        //
        // IMPORTANT: this happens BEFORE `mark_accepted`. If the quota
        // is exhausted, the invitation remains pending — the user can
        // retry later after the tenant frees up a slot.
        if let Some(resolver) = &self.entitlement_resolver {
            let entitlements = resolver
                .resolve(invitation.tenant_id)
                .await
                .map_err(InvitationError::Internal)?;

            match self
                .membership_repository
                .save_if_within_quota(
                    &membership,
                    invitation.tenant_id,
                    entitlements.max_team_members,
                )
                .await
                .map_err(InvitationError::Internal)?
            {
                QuotaCheckOutcome::Saved => {}
                QuotaCheckOutcome::QuotaExceeded => {
                    return Err(InvitationError::Internal(Box::new(PlanLimitError::new(
                        MAX_TEAM_MEMBERS,
                        u64::from(entitlements.max_team_members),
                        u64::from(entitlements.max_team_members),
                        "upgrade_plan",
                    ))));
                }
            }
        } else {
            // Backward-compatible path: no resolver wired → plain
            // create (pre-TASK-4A behaviour).
            self.membership_repository
                .create(&membership)
                .await
                .map_err(InvitationError::Internal)?;
        }

        // Mark the invitation consumed only after the membership exists.
        if let Err(e) = self
            .invitation_repository
            .mark_accepted(invitation.id, now)
            .await
        {
            // Membership was already persisted; invitation stays
            // pending but the user is a member. Not fatal — retrying
            // the same token hits `find_by_tenant_and_user` above.
            tracing::warn!(
                error = %e,
                invitation_id = %invitation.id.into_uuid(),
                "failed to mark invitation accepted after membership creation"
            );
        }

        Ok(membership)
    }
}
