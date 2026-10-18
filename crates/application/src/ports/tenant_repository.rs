use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::{
    Tenant,
    shared::ids::{TenantId, UserId},
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for `Tenant`.
///
/// Deliberately limited to create/read plus `list_all` for background
/// background workers. There is still no `update` or `delete` here because
/// tenant deletion is a much larger lifecycle operation.
#[async_trait]
pub trait TenantRepository: Send + Sync {
    async fn create(&self, tenant: &Tenant) -> RepoResult<()>;

    async fn find_by_id(&self, id: TenantId) -> RepoResult<Option<Tenant>>;

    async fn find_by_owner_user_id(&self, owner_user_id: UserId) -> RepoResult<Option<Tenant>>;

    async fn list_all(&self) -> RepoResult<Vec<Tenant>>;
    async fn set_status(&self, _id: TenantId, _status: domain::TenantStatus) -> RepoResult<bool> {
        Ok(false)
    }
}

#[async_trait]
impl<T> TenantRepository for Arc<T>
where
    T: TenantRepository + Send + Sync + ?Sized,
{
    async fn create(&self, tenant: &Tenant) -> RepoResult<()> {
        (**self).create(tenant).await
    }

    async fn find_by_id(&self, id: TenantId) -> RepoResult<Option<Tenant>> {
        (**self).find_by_id(id).await
    }

    async fn find_by_owner_user_id(&self, owner_user_id: UserId) -> RepoResult<Option<Tenant>> {
        (**self).find_by_owner_user_id(owner_user_id).await
    }

    async fn list_all(&self) -> RepoResult<Vec<Tenant>> {
        (**self).list_all().await
    }
    async fn set_status(&self, id: TenantId, status: domain::TenantStatus) -> RepoResult<bool> {
        (**self).set_status(id, status).await
    }
}
