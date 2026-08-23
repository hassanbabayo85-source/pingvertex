use std::sync::Arc;

use domain::{
    TenantMembership,
    shared::types::{AuthenticatedUser, Permission},
};

use crate::ports::TenantMembershipRepository;

use super::membership_common::MembershipError;

/// Lists all members of the caller's current tenant.
///
/// Requires `Permission::ManageUsers` (Owner or Admin).
#[derive(Clone)]
pub struct ListMembersUseCase {
    tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
}

impl ListMembersUseCase {
    pub fn new(tenant_membership_repository: Arc<dyn TenantMembershipRepository>) -> Self {
        Self {
            tenant_membership_repository,
        }
    }

    pub async fn execute(
        &self,
        actor: AuthenticatedUser,
    ) -> Result<Vec<TenantMembership>, MembershipError> {
        if !actor.can(Permission::ManageUsers) {
            return Err(MembershipError::Forbidden);
        }

        let tenant_id = actor.tenant_context().tenant_id();
        self.tenant_membership_repository
            .list_by_tenant(tenant_id)
            .await
            .map_err(|err| MembershipError::Other(err))
    }
}
