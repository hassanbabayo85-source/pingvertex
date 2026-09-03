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

    /// Saves multiple checks atomically where the backing store supports
    /// it (e.g. a single multi-row INSERT on Postgres). The default
    /// implementation falls back to individual saves, so existing
    /// in-memory/mock implementations continue to work unchanged.
    async fn save_batch(&self, checks: &[Check]) -> RepoResult<()> {
        for check in checks {
            self.save(check).await?;
        }
        Ok(())
    }

    /// AUDIT ISSUE #1 fix: `Check`'s canonical identity is the composite
    /// `(CheckId, checked_at)`, matching `checks`' actual PRIMARY KEY
    /// since migration 0061 partitioned the table by `checked_at`. See
    /// `domain::repositories::CheckRepository::find_by_id`'s doc comment
    /// for the full rationale.
    async fn find_by_id(&self, id: CheckId, checked_at: DateTime<Utc>)
    -> RepoResult<Option<Check>>;

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

    async fn save_batch(&self, checks: &[Check]) -> RepoResult<()> {
        (**self).save_batch(checks).await
    }

    async fn find_by_id(
        &self,
        id: CheckId,
        checked_at: DateTime<Utc>,
    ) -> RepoResult<Option<Check>> {
        (**self).find_by_id(id, checked_at).await
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
