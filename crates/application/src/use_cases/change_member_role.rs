use std::sync::Arc;

use domain::shared::{
    ids::UserId,
    types::{AuthenticatedUser, Permission, Role},
};

use crate::ports::TenantMembershipRepository;

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

        // Last-owner guard for self-demotion
        if actor.user_id() == command.user_id
            && target.role == Role::Owner
            && command.new_role != Role::Owner
        {
            let owners = self
                .tenant_membership_repository
                .list_by_tenant(tenant_id)
                .await
                .map_err(MembershipError::Other)?
                .into_iter()
                .filter(|m| m.role == Role::Owner)
                .count();
            if owners <= 1 {
                return Err(MembershipError::InvalidOperation(
                    "cannot demote the last Owner".to_string(),
                ));
            }
        }

        self.tenant_membership_repository
            .update_role(tenant_id, command.user_id, command.new_role)
            .await
            .map_err(MembershipError::Other)
    }
}
