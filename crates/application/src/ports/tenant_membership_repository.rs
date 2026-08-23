use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use domain::{
    TenantMembership,
    shared::{
        ids::{TenantId, UserId},
        types::Role,
    },
};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for `TenantMembership`.
///
/// `find_by_tenant_and_user` is deliberately first-class here because
/// it is used by the authorization middleware to resolve a user's
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

    /// Deletes a membership row. Returns an error if the row does not exist
    /// is left to the caller; this method performs a `DELETE ... WHERE ...`
    /// and does not error when 0 rows were affected.
    async fn delete(&self, tenant_id: TenantId, user_id: UserId) -> RepoResult<()>;
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

    async fn delete(&self, tenant_id: TenantId, user_id: UserId) -> RepoResult<()> {
        (**self).delete(tenant_id, user_id).await
    }
}
