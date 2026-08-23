use std::sync::Arc;

use chrono::Utc;
use domain::{
    TenantMembership,
    shared::{
        ids::UserId,
        types::{AuthenticatedUser, Permission, Role},
    },
};

use crate::ports::{TenantMembershipRepository, UserRepository};

use super::membership_common::MembershipError;

pub struct AddMemberCommand {
    pub user_id: UserId,
    pub role: Role,
}

/// Adds a user directly to the caller's tenant with the given role.
///
/// Authorization:
/// - Requires `Permission::ManageUsers`.
/// - Only `Owner` may assign `Owner` or `Admin`.
/// - `Owner` or `Admin` may assign `Member`, `Viewer`, or `Auditor`.
#[derive(Clone)]
pub struct AddMemberUseCase {
    tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
    user_repository: Arc<dyn UserRepository>,
}

impl AddMemberUseCase {
    pub fn new(
        tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
        user_repository: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            tenant_membership_repository,
            user_repository,
        }
    }

    pub async fn execute(
        &self,
        command: AddMemberCommand,
        actor: AuthenticatedUser,
    ) -> Result<TenantMembership, MembershipError> {
        if !actor.can(Permission::ManageUsers) {
            return Err(MembershipError::Forbidden);
        }

        let actor_role = actor.role();
        match command.role {
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

        // Verify the target user exists.
        if self
            .user_repository
            .find_by_id(&command.user_id)
            .await
            .map_err(|err| MembershipError::Other(err))?
            .is_none()
        {
            return Err(MembershipError::NotFound);
        }

        let tenant_id = actor.tenant_context().tenant_id();
        if self
            .tenant_membership_repository
            .find_by_tenant_and_user(tenant_id, command.user_id)
            .await
            .map_err(|err| MembershipError::Other(err))?
            .is_some()
        {
            return Err(MembershipError::Conflict);
        }

        let now = Utc::now();
        let membership = TenantMembership {
            tenant_id,
            user_id: command.user_id,
            role: command.role,
            invited_at: now,
            joined_at: Some(now),
        };

        self.tenant_membership_repository
            .create(&membership)
            .await
            .map_err(|err| MembershipError::Other(err))?;

        Ok(membership)
    }
}
