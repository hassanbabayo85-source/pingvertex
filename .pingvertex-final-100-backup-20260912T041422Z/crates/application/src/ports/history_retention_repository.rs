use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::TenantId;
use std::{error::Error, sync::Arc};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Port for tenant-scoped, batched check-history deletion.
///
/// Kept separate from `CheckRepository` because regular application
/// reads/writes never bulk-delete history; only the retention worker
/// does. Implementations must delete one bounded batch at a time to
/// avoid long lock/transaction times.
#[async_trait]
pub trait HistoryRetentionRepository: Send + Sync {
    /// Deletes up to `batch_size` check rows older than `cutoff` for
    /// `tenant_id`. Returns the number of rows actually deleted.
    async fn delete_older_than_batch(
        &self,
        tenant_id: TenantId,
        cutoff: DateTime<Utc>,
        batch_size: u32,
    ) -> RepoResult<u64>;

    /// Deletes all check rows older than `cutoff` for `tenant_id` by
    /// repeatedly calling `delete_older_than_batch` until no more rows
    /// match. The default implementation is crash-safe/resumable because
    /// each batch is an independent, idempotent delete.
    async fn delete_older_than_batched(
        &self,
        tenant_id: TenantId,
        cutoff: DateTime<Utc>,
        batch_size: u32,
    ) -> RepoResult<u64> {
        let mut total_deleted = 0u64;
        loop {
            let deleted = self
                .delete_older_than_batch(tenant_id, cutoff, batch_size)
                .await?;
            if deleted == 0 {
                break;
            }
            total_deleted += deleted;
        }
        Ok(total_deleted)
    }
}

#[async_trait]
impl<T> HistoryRetentionRepository for Arc<T>
where
    T: HistoryRetentionRepository + Send + Sync + ?Sized,
{
    async fn delete_older_than_batch(
        &self,
        tenant_id: TenantId,
        cutoff: DateTime<Utc>,
        batch_size: u32,
    ) -> RepoResult<u64> {
        (**self)
            .delete_older_than_batch(tenant_id, cutoff, batch_size)
            .await
    }
}
