use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use domain::{NotificationOutboxEntry, NotificationOutboxEntryId, NotificationOutboxStatus};
use std::{error::Error, sync::Arc};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence for the notification outbox.
///
/// Implementations are expected to make `claim_next` atomic under
/// concurrency (`FOR UPDATE SKIP LOCKED` in Postgres) so multiple worker
/// processes can safely claim disjoint rows.
#[async_trait]
pub trait NotificationOutboxRepository: Send + Sync {
    /// Atomically insert an outbox entry.
    ///
    /// Returns `Ok(true)` when the entry was inserted, or `Ok(false)` when
    /// another row already used the same `idempotency_key` and this row was
    /// ignored. This is the outbox deduplication mechanism.
    async fn enqueue(&self, entry: &NotificationOutboxEntry) -> RepoResult<bool>;

    /// Return every stale `Processing` row to `Pending`.
    ///
    /// A row is stale when its `processing_started_at` is older than
    /// `now() - timeout`. This is how a worker process crash is recovered
    /// without operator intervention.
    async fn recover_stale_processing(&self, timeout: Duration) -> RepoResult<u64>;

    /// Claim the oldest due `Pending` row and mark it `Processing`.
    async fn claim_next(&self) -> RepoResult<Option<NotificationOutboxEntry>>;

    /// Mark a claimed row as successfully delivered.
    async fn mark_sent(&self, id: NotificationOutboxEntryId) -> RepoResult<()>;

    /// Record a failed delivery attempt.
    ///
    /// Increments `attempts` atomically. If the new attempt count has
    /// reached `max_attempts`, the row becomes permanently `Failed`;
    /// otherwise it returns to `Pending` with the supplied `retry_at`.
    /// Returns the resulting status.
    async fn mark_failed(
        &self,
        id: NotificationOutboxEntryId,
        error: String,
        retry_at: DateTime<Utc>,
        max_attempts: i32,
    ) -> RepoResult<NotificationOutboxStatus>;
}

#[async_trait]
impl<T> NotificationOutboxRepository for Arc<T>
where
    T: NotificationOutboxRepository + Send + Sync + ?Sized,
{
    async fn enqueue(&self, entry: &NotificationOutboxEntry) -> RepoResult<bool> {
        (**self).enqueue(entry).await
    }

    async fn recover_stale_processing(&self, timeout: Duration) -> RepoResult<u64> {
        (**self).recover_stale_processing(timeout).await
    }

    async fn claim_next(&self) -> RepoResult<Option<NotificationOutboxEntry>> {
        (**self).claim_next().await
    }

    async fn mark_sent(&self, id: NotificationOutboxEntryId) -> RepoResult<()> {
        (**self).mark_sent(id).await
    }

    async fn mark_failed(
        &self,
        id: NotificationOutboxEntryId,
        error: String,
        retry_at: DateTime<Utc>,
        max_attempts: i32,
    ) -> RepoResult<NotificationOutboxStatus> {
        (**self)
            .mark_failed(id, error, retry_at, max_attempts)
            .await
    }
}
