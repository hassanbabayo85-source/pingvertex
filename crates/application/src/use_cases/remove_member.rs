use std::sync::Arc;

use domain::shared::{
    ids::UserId,
    types::{AuthenticatedUser, Permission, Role},
};

use crate::ports::TenantMembershipRepository;

use super::membership_common::MembershipError;

/// Removes a member from the caller's current tenant.
///
/// Authorization:
/// - Requires `Permission::ManageUsers`.
/// - Caller cannot remove themselves.
/// - Only `Owner` may remove an `Owner` or `Admin`.
/// - `Owner` or `Admin` may remove `Member`, `Viewer`, or `Auditor`.
/// - The last `Owner` cannot be removed.
#[derive(Clone)]
pub struct RemoveMemberUseCase {
    tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
}

impl RemoveMemberUseCase {
    pub fn new(tenant_membership_repository: Arc<dyn TenantMembershipRepository>) -> Self {
        Self {
            tenant_membership_repository,
        }
    }

    pub async fn execute(
        &self,
        target_user_id: UserId,
        actor: AuthenticatedUser,
    ) -> Result<(), MembershipError> {
        if !actor.can(Permission::ManageUsers) {
            return Err(MembershipError::Forbidden);
        }

        if actor.user_id() == target_user_id {
            return Err(MembershipError::InvalidOperation(
                "cannot remove your own membership".to_string(),
            ));
        }

        let tenant_id = actor.tenant_context().tenant_id();
        let target = self
            .tenant_membership_repository
            .find_by_tenant_and_user(tenant_id, target_user_id)
            .await
            .map_err(MembershipError::Other)?
            .ok_or(MembershipError::NotFound)?;

        let actor_role = actor.role();
        match target.role {
            Role::Owner | Role::Admin => {
                if actor_role != Role::Owner {
                    return Err(MembershipError::Forbidden);
                }
            }
            Role::Member | Role::Viewer | Role::Auditor => {
                if !matches!(actor_role, Role::Owner | Role::Admin) {
                    return Err(MembershipError::Forbidden);
                }
            }
        }

        // Guard against removing the last Owner.
        if target.role == Role::Owner {
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
                    "cannot remove the last Owner".to_string(),
                ));
            }
        }

        self.tenant_membership_repository
            .delete(tenant_id, target_user_id)
            .await
            .map_err(MembershipError::Other)
    }
}
