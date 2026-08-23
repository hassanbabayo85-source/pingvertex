use async_trait::async_trait;
use std::{error::Error, sync::Arc};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Idempotency-key persistence for billing webhook deliveries.
///
/// The repository only inserts keys. There is deliberately no
/// `delete` or `update`: once a provider event id has been seen, it must
/// never be re-processed, and the key must never be removed.
#[async_trait]
pub trait BillingWebhookIdempotencyRepository: Send + Sync {
    /// Atomically insert `provider_event_id`.
    ///
    /// Returns `Ok(true)` if the id was newly inserted and processing may
    /// continue. Returns `Ok(false)` if the id already existed, meaning
    /// this is a duplicate delivery and effects must NOT be applied again.
    async fn try_insert(&self, provider_event_id: &str) -> RepoResult<bool>;
}

#[async_trait]
impl<T> BillingWebhookIdempotencyRepository for Arc<T>
where
    T: BillingWebhookIdempotencyRepository + Send + Sync + ?Sized,
{
    async fn try_insert(&self, provider_event_id: &str) -> RepoResult<bool> {
        (**self).try_insert(provider_event_id).await
    }
}
