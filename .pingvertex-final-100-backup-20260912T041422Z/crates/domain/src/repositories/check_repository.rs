use std::error::Error;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::shared::ids::{CheckId, MonitorId};

use crate::entities::Check;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait CheckRepository: Send + Sync {
    async fn save(&self, check: &Check) -> RepoResult<()>;

    /// AUDIT ISSUE #1 fix: looks up a `Check` by its canonical identity.
    ///
    /// `checks.id` alone is NOT guaranteed unique at the database level
    /// since migration 0061 partitioned `checks` by `checked_at` (a
    /// partitioned table's primary key must include the partition key,
    /// so the primary key became the composite `(id, checked_at)`). The
    /// canonical `Check` identity is therefore `(CheckId, checked_at)`,
    /// not `CheckId` alone — every caller must supply both.
    async fn find_by_id(&self, id: CheckId, checked_at: DateTime<Utc>)
    -> RepoResult<Option<Check>>;

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Check>>;

    async fn list_checks(&self, limit: u32, offset: u32) -> RepoResult<Vec<Check>>;

    async fn delete_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<u64>;
}
