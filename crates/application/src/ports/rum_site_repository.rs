use std::error::Error;

use crate::pagination::{Cursor, Page, PageRequest, paginate_in_memory};
use async_trait::async_trait;
use domain::RumSite;
use domain::shared::ids::{RumSiteId, TenantId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence for `RumSite` (the sites/properties a tenant has RUM
/// instrumentation installed on).
///
/// `find_by_site_key` is the ONLY lookup the ingestion path (Task C) is
/// allowed to use to resolve a tenant from an inbound beacon — a beacon
/// carries a `site_key`, never a `tenant_id` directly, so tenant_id is
/// always derived server-side from this lookup and never trusted from
/// client input. See `IngestRumEventUseCase`.
#[async_trait]
pub trait RumSiteRepository: Send + Sync {
    async fn create(&self, site: &RumSite) -> RepoResult<()>;

    async fn find_by_site_key(&self, site_key: &str) -> RepoResult<Option<RumSite>>;

    async fn find_by_id(&self, id: RumSiteId, tenant_id: TenantId) -> RepoResult<Option<RumSite>>;

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<RumSite>>;

    /// PART 8 — cursor-paginated variant of `list_by_tenant`.
    async fn list_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<RumSite>> {
        let all = self.list_by_tenant(tenant_id).await?;
        Ok(paginate_in_memory(all, page, |s| {
            Cursor::new(s.created_at, s.id.0)
        }))
    }
}

#[async_trait]
impl<T> RumSiteRepository for std::sync::Arc<T>
where
    T: RumSiteRepository + ?Sized,
{
    async fn create(&self, site: &RumSite) -> RepoResult<()> {
        (**self).create(site).await
    }

    async fn find_by_site_key(&self, site_key: &str) -> RepoResult<Option<RumSite>> {
        (**self).find_by_site_key(site_key).await
    }

    async fn find_by_id(&self, id: RumSiteId, tenant_id: TenantId) -> RepoResult<Option<RumSite>> {
        (**self).find_by_id(id, tenant_id).await
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<RumSite>> {
        (**self).list_by_tenant(tenant_id).await
    }

    async fn list_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<RumSite>> {
        (**self).list_by_tenant_paginated(tenant_id, page).await
    }
}
