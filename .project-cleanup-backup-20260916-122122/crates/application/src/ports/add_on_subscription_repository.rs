use async_trait::async_trait;

use crate::pagination::{Cursor, Page, PageRequest, paginate_in_memory};
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
    async fn update(&self, a: &AddOnSubscription) -> RepoResult<()>;

    /// PART 8 — cursor-paginated variant of `list_active_by_tenant`.
    async fn list_active_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<AddOnSubscription>> {
        let all = self.list_active_by_tenant(tenant_id).await?;
        Ok(paginate_in_memory(all, page, |a| {
            Cursor::new(a.created_at, a.id.0)
        }))
    }

    /// PART 8 — cursor-paginated variant of `list_by_tenant_and_type`.
    async fn list_by_tenant_and_type_paginated(
        &self,
        tenant_id: TenantId,
        addon_type: AddOnType,
        page: PageRequest,
    ) -> RepoResult<Page<AddOnSubscription>> {
        let all = self.list_by_tenant_and_type(tenant_id, addon_type).await?;
        Ok(paginate_in_memory(all, page, |a| {
            Cursor::new(a.created_at, a.id.0)
        }))
    }
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
    async fn update(&self, a: &AddOnSubscription) -> RepoResult<()> {
        (**self).update(a).await
    }

    async fn list_active_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<AddOnSubscription>> {
        (**self)
            .list_active_by_tenant_paginated(tenant_id, page)
            .await
    }

    async fn list_by_tenant_and_type_paginated(
        &self,
        tenant_id: TenantId,
        addon_type: AddOnType,
        page: PageRequest,
    ) -> RepoResult<Page<AddOnSubscription>> {
        (**self)
            .list_by_tenant_and_type_paginated(tenant_id, addon_type, page)
            .await
    }
}
