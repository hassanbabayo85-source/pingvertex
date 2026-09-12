use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::BillingAuditLogEntry;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Append-only persistence port for billing audit log entries.
///
/// Deliberately has NO `update` and NO `delete` methods. Billing events
/// are recorded once and never mutated.
#[async_trait]
pub trait BillingAuditLogRepository: Send + Sync {
    async fn create(&self, entry: &BillingAuditLogEntry) -> RepoResult<()>;
}

#[async_trait]
impl<T> BillingAuditLogRepository for Arc<T>
where
    T: BillingAuditLogRepository + Send + Sync + ?Sized,
{
    async fn create(&self, entry: &BillingAuditLogEntry) -> RepoResult<()> {
        (**self).create(entry).await
    }
}
