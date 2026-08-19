use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::{
    TenantMembership,
    shared::{
        ids::{TenantId, UserId},
        types::Role,
    },
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for `TenantMembership`.
///
/// `find_by_tenant_and_user` is deliberately first-class here because
/// `jwt_auth` (a later worker) will use it to build the request-scoped
/// `TenantContext` and `Role` from the authenticated `UserId`.
///
/// `update_role` exists now even if this feature does not call it yet,
/// so the future `RoleChanged` audit action has a port to write through.
#[async_trait]
pub trait TenantMembershipRepository: Send + Sync {
    async fn create(&self, membership: &TenantMembership) -> RepoResult<()>;

    async fn find_by_tenant_and_user(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<Option<TenantMembership>>;

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<TenantMembership>>;

    async fn list_by_user(&self, user_id: UserId) -> RepoResult<Vec<TenantMembership>>;

    async fn update_role(&self, tenant_id: TenantId, user_id: UserId, role: Role)
    -> RepoResult<()>;
}

#[async_trait]
impl<T> TenantMembershipRepository for Arc<T>
where
    T: TenantMembershipRepository + Send + Sync + ?Sized,
{
    async fn create(&self, membership: &TenantMembership) -> RepoResult<()> {
        (**self).create(membership).await
    }

    async fn find_by_tenant_and_user(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<Option<TenantMembership>> {
        (**self).find_by_tenant_and_user(tenant_id, user_id).await
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<TenantMembership>> {
        (**self).list_by_tenant(tenant_id).await
    }

    async fn list_by_user(&self, user_id: UserId) -> RepoResult<Vec<TenantMembership>> {
        (**self).list_by_user(user_id).await
    }

    async fn update_role(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        role: Role,
    ) -> RepoResult<()> {
        (**self).update_role(tenant_id, user_id, role).await
    }
}
