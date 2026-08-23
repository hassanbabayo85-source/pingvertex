use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::Check;
use domain::CheckStatus;
use domain::shared::ids::{CheckId, MonitorId, TenantId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Filters for check-history queries.
#[derive(Debug, Clone, Default)]
pub struct CheckFilters {
    pub monitor_id: Option<MonitorId>,
    pub status: Option<CheckStatus>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait CheckRepository: Send + Sync {
    async fn save(&self, check: &Check) -> RepoResult<()>;

    async fn find_by_id(&self, id: CheckId) -> RepoResult<Option<Check>>;

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Check>>;

    async fn list_checks(
        &self,
        limit: u32,
        offset: u32,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<Check>>;

    /// Tenant-scoped, filtered, paginated check history.
    async fn list_checks_filtered(
        &self,
        limit: u32,
        offset: u32,
        tenant_id: TenantId,
        filters: CheckFilters,
    ) -> RepoResult<Vec<Check>> {
        let _ = (limit, offset, tenant_id, filters);
        Ok(vec![])
    }

    /// Monitor-scoped, filtered, paginated check history.
    async fn list_by_monitor_paginated(
        &self,
        monitor_id: MonitorId,
        limit: u32,
        offset: u32,
        filters: CheckFilters,
    ) -> RepoResult<Vec<Check>> {
        let _ = (monitor_id, limit, offset, filters);
        Ok(vec![])
    }

    async fn delete_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<u64>;
}

#[async_trait]
impl<T> CheckRepository for Arc<T>
where
    T: CheckRepository + ?Sized + Send + Sync,
{
    async fn save(&self, check: &Check) -> RepoResult<()> {
        (**self).save(check).await
    }

    async fn find_by_id(&self, id: CheckId) -> RepoResult<Option<Check>> {
        (**self).find_by_id(id).await
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Check>> {
        (**self).list_by_monitor(monitor_id).await
    }

    async fn list_checks(
        &self,
        limit: u32,
        offset: u32,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<Check>> {
        (**self).list_checks(limit, offset, tenant_id).await
    }

    async fn list_checks_filtered(
        &self,
        limit: u32,
        offset: u32,
        tenant_id: TenantId,
        filters: CheckFilters,
    ) -> RepoResult<Vec<Check>> {
        (**self)
            .list_checks_filtered(limit, offset, tenant_id, filters)
            .await
    }

    async fn list_by_monitor_paginated(
        &self,
        monitor_id: MonitorId,
        limit: u32,
        offset: u32,
        filters: CheckFilters,
    ) -> RepoResult<Vec<Check>> {
        (**self)
            .list_by_monitor_paginated(monitor_id, limit, offset, filters)
            .await
    }

    async fn delete_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<u64> {
        (**self).delete_by_monitor(monitor_id).await
    }
}
