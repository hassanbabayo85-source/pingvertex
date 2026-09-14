use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

use crate::pagination::{Cursor, Page, PageRequest, paginate_in_memory};
use domain::EscalationPolicy;
use domain::shared::ids::{EscalationPolicyId, MonitorId, TenantId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait EscalationPolicyRepository: Send + Sync {
    async fn create(&self, policy: &EscalationPolicy) -> RepoResult<()>;

    async fn update(&self, policy: &EscalationPolicy) -> RepoResult<()>;

    /// Returns the effective policy for a monitor: monitor-specific first,
    /// then the user's global policy (monitor_id IS NULL).
    async fn find_effective(
        &self,
        monitor_id: MonitorId,
        tenant_id: TenantId,
    ) -> RepoResult<Option<EscalationPolicy>>;

    /// Fetches a single policy with NO ownership filtering. For callers
    /// that authorize the result themselves against the caller's tenant
    /// (e.g. via `AuthorizationPolicy::same_tenant`) instead of the
    /// single-owner `find_by_id_for_user` check.
    async fn find_by_id(&self, _id: EscalationPolicyId) -> RepoResult<Option<EscalationPolicy>> {
        unimplemented!()
    }

    /// Tenant-scoped equivalent of `list_for_user`. Returns every policy
    /// belonging to `tenant_id`, regardless of which member of the
    /// tenant created it.
    async fn list_for_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<EscalationPolicy>> {
        unimplemented!()
    }

    /// PART 8 — cursor-paginated variant of `list_for_tenant`.
    async fn list_for_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<EscalationPolicy>> {
        let all = self.list_for_tenant(tenant_id).await?;
        Ok(paginate_in_memory(all, page, |p| {
            Cursor::new(p.created_at, p.id.0)
        }))
    }
}

#[async_trait]
impl<T> EscalationPolicyRepository for Arc<T>
where
    T: EscalationPolicyRepository + Send + Sync + ?Sized,
{
    async fn create(&self, policy: &EscalationPolicy) -> RepoResult<()> {
        (**self).create(policy).await
    }

    async fn update(&self, policy: &EscalationPolicy) -> RepoResult<()> {
        (**self).update(policy).await
    }

    async fn find_effective(
        &self,
        monitor_id: MonitorId,
        tenant_id: TenantId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        (**self).find_effective(monitor_id, tenant_id).await
    }

    async fn find_by_id(&self, id: EscalationPolicyId) -> RepoResult<Option<EscalationPolicy>> {
        (**self).find_by_id(id).await
    }

    async fn list_for_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<EscalationPolicy>> {
        (**self).list_for_tenant(tenant_id).await
    }

    async fn list_for_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<EscalationPolicy>> {
        (**self).list_for_tenant_paginated(tenant_id, page).await
    }
}
