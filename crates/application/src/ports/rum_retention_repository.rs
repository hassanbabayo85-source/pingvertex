use std::error::Error;

use async_trait::async_trait;
use domain::shared::ids::{RumSiteId, TenantId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Outcome of one site's retention pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RumRetentionOutcome {
    pub days_aggregated: u64,
    pub page_views_deleted: u64,
    pub js_errors_deleted: u64,
    pub resource_errors_deleted: u64,
    pub sessions_deleted: u64,
}

/// RUM retention: roll raw events older than `raw_retention_days` into
/// `rum_daily_aggregates`, then delete the raw rows in batches.
///
/// Implementations MUST aggregate BEFORE deleting — the aggregate is the
/// permanent record of days that fall out of the raw retention window.
/// Deletion is batched to bound lock/transaction time.
#[async_trait]
pub trait RumRetentionRepository: Send + Sync {
    /// For one site: aggregate every day older than `raw_retention_days`
    /// into `rum_daily_aggregates` (idempotent, `ON CONFLICT DO UPDATE`),
    /// then delete the raw rows for those days in batches of `batch_size`.
    async fn run_retention_for_site(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        raw_retention_days: i64,
        batch_size: u32,
    ) -> RepoResult<RumRetentionOutcome>;
}

#[async_trait]
impl<T> RumRetentionRepository for std::sync::Arc<T>
where
    T: RumRetentionRepository + ?Sized,
{
    async fn run_retention_for_site(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        raw_retention_days: i64,
        batch_size: u32,
    ) -> RepoResult<RumRetentionOutcome> {
        (**self)
            .run_retention_for_site(tenant_id, site_id, raw_retention_days, batch_size)
            .await
    }
}
