use std::sync::Arc;

use domain::shared::{
    ids::UserId,
    types::{AuthenticatedUser, Permission, Role},
};

use crate::ports::{LastOwnerGuardOutcome, TenantMembershipRepository};

use super::membership_common::MembershipError;

pub struct ChangeMemberRoleCommand {
    pub user_id: UserId,
    pub new_role: Role,
}

/// Changes a member's role in the caller's current tenant.
///
/// Authorization:
/// - Requires `Permission::ManageUsers`.
/// - `Owner` can set any role, but cannot demote themselves if they are
///   the last Owner.
/// - `Admin` can only set `Member`, `Viewer`, or `Auditor` on members whose
///   current role is also `Member`, `Viewer`, or `Auditor`. They cannot
///   modify `Owner` or `Admin`.
#[derive(Clone)]
pub struct ChangeMemberRoleUseCase {
    tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
}

impl ChangeMemberRoleUseCase {
    pub fn new(tenant_membership_repository: Arc<dyn TenantMembershipRepository>) -> Self {
        Self {
            tenant_membership_repository,
        }
    }

    pub async fn execute(
        &self,
        command: ChangeMemberRoleCommand,
        actor: AuthenticatedUser,
    ) -> Result<(), MembershipError> {
        if !actor.can(Permission::ManageUsers) {
            return Err(MembershipError::Forbidden);
        }

        let tenant_id = actor.tenant_context().tenant_id();
        let target = self
            .tenant_membership_repository
            .find_by_tenant_and_user(tenant_id, command.user_id)
            .await
            .map_err(MembershipError::Other)?
            .ok_or(MembershipError::NotFound)?;

        let actor_role = actor.role();

        // Admin restrictions
        if actor_role == Role::Admin
            && (matches!(target.role, Role::Owner | Role::Admin)
                || matches!(command.new_role, Role::Owner | Role::Admin))
        {
            return Err(MembershipError::Forbidden);
        }

        // Only Owner can assign Owner/Admin (already covered above, but keep explicit)
        if matches!(command.new_role, Role::Owner | Role::Admin) && actor_role != Role::Owner {
            return Err(MembershipError::Forbidden);
        }

        // The last-Owner guard applies whenever an Owner is being moved
        // to a non-Owner role — not only on self-demotion. (An Owner
        // demoting a *peer* Owner is just as capable of leaving the
        // tenant ownerless as self-demotion is, once two such calls race
        // each other.) The check and the write happen atomically in the
        // repository (row-locked transaction for Postgres, single mutex
        // critical section in-memory), which is what actually closes the
        // race: two concurrent calls can no longer both observe
        // "owners > 1" and both proceed.
        match self
            .tenant_membership_repository
            .update_role_if_not_last_owner(tenant_id, command.user_id, command.new_role)
            .await
            .map_err(MembershipError::Other)?
        {
            LastOwnerGuardOutcome::Applied => Ok(()),
            LastOwnerGuardOutcome::NotFound => Err(MembershipError::NotFound),
            LastOwnerGuardOutcome::WouldRemoveLastOwner => Err(MembershipError::InvalidOperation(
                "cannot demote the last Owner".to_string(),
            )),
        }
    }
}
