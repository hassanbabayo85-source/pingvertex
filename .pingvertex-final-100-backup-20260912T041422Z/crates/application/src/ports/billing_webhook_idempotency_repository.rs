use async_trait::async_trait;
use std::{error::Error, sync::Arc, time::Duration};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookIdempotencyClaim {
    Claimed,
    AlreadyProcessed,
    InProgress,
}

#[async_trait]
pub trait BillingWebhookIdempotencyRepository: Send + Sync {
    async fn try_insert(&self, provider_event_id: &str) -> RepoResult<bool>;

    async fn try_claim(
        &self,
        provider_event_id: &str,
        _stale_timeout: Duration,
    ) -> RepoResult<WebhookIdempotencyClaim> {
        if self.try_insert(provider_event_id).await? {
            Ok(WebhookIdempotencyClaim::Claimed)
        } else {
            Ok(WebhookIdempotencyClaim::AlreadyProcessed)
        }
    }

    async fn mark_processed(&self, provider_event_id: &str) -> RepoResult<()> {
        let _ = provider_event_id;
        Ok(())
    }

    async fn mark_pending(&self, provider_event_id: &str) -> RepoResult<()> {
        let _ = provider_event_id;
        Ok(())
    }
}

#[async_trait]
impl<T> BillingWebhookIdempotencyRepository for Arc<T>
where
    T: BillingWebhookIdempotencyRepository + Send + Sync + ?Sized,
{
    async fn try_insert(&self, provider_event_id: &str) -> RepoResult<bool> {
        (**self).try_insert(provider_event_id).await
    }

    async fn try_claim(
        &self,
        provider_event_id: &str,
        stale_timeout: Duration,
    ) -> RepoResult<WebhookIdempotencyClaim> {
        (**self).try_claim(provider_event_id, stale_timeout).await
    }

    async fn mark_processed(&self, provider_event_id: &str) -> RepoResult<()> {
        (**self).mark_processed(provider_event_id).await
    }

    async fn mark_pending(&self, provider_event_id: &str) -> RepoResult<()> {
        (**self).mark_pending(provider_event_id).await
    }
}
