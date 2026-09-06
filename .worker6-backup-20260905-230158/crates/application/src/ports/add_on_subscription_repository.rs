use async_trait::async_trait;
use domain::entities::{AddOnSubscription, AddOnType};
use domain::shared::ids::AddOnSubscriptionId;
use domain::shared::ids::TenantId;
use std::{error::Error, sync::Arc};
pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
#[async_trait]
pub trait AddOnSubscriptionRepository: Send + Sync {
    async fn create(&self, a: &AddOnSubscription) -> RepoResult<()>;
    async fn find_by_id(&self, id: AddOnSubscriptionId) -> RepoResult<Option<AddOnSubscription>>;
    async fn list_active_by_tenant(&self, t: TenantId) -> RepoResult<Vec<AddOnSubscription>>;
    async fn list_by_tenant_and_type(
        &self,
        t: TenantId,
        k: AddOnType,
    ) -> RepoResult<Vec<AddOnSubscription>>;
}
#[async_trait]
impl<T> AddOnSubscriptionRepository for Arc<T>
where
    T: AddOnSubscriptionRepository + Send + Sync + ?Sized,
{
    async fn create(&self, a: &AddOnSubscription) -> RepoResult<()> {
        (**self).create(a).await
    }
    async fn find_by_id(&self, id: AddOnSubscriptionId) -> RepoResult<Option<AddOnSubscription>> {
        (**self).find_by_id(id).await
    }
    async fn list_active_by_tenant(&self, t: TenantId) -> RepoResult<Vec<AddOnSubscription>> {
        (**self).list_active_by_tenant(t).await
    }
    async fn list_by_tenant_and_type(
        &self,
        t: TenantId,
        k: AddOnType,
    ) -> RepoResult<Vec<AddOnSubscription>> {
        (**self).list_by_tenant_and_type(t, k).await
    }
}
