use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use application::ports::BillingWebhookIdempotencyRepository;
use application::ports::billing_webhook_idempotency_repository::WebhookIdempotencyClaim;
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
enum InMemoryWebhookStatus {
    Pending,
    Processing { started_at: Instant },
    Processed,
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryBillingWebhookIdempotencyRepository {
    statuses: Arc<Mutex<HashMap<String, InMemoryWebhookStatus>>>,
}

impl InMemoryBillingWebhookIdempotencyRepository {
    pub fn new() -> Self {
        Self {
            statuses: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl BillingWebhookIdempotencyRepository for InMemoryBillingWebhookIdempotencyRepository {
    async fn try_insert(
        &self,
        provider_event_id: &str,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let mut statuses = self.statuses.lock().unwrap();

        if statuses.contains_key(provider_event_id) {
            return Ok(false);
        }

        statuses.insert(provider_event_id.to_owned(), InMemoryWebhookStatus::Pending);
        Ok(true)
    }

    async fn try_claim(
        &self,
        provider_event_id: &str,
        stale_timeout: Duration,
    ) -> Result<WebhookIdempotencyClaim, Box<dyn Error + Send + Sync>> {
        let mut statuses = self.statuses.lock().unwrap();

        match statuses.get(provider_event_id) {
            None => {
                statuses.insert(
                    provider_event_id.to_owned(),
                    InMemoryWebhookStatus::Processing {
                        started_at: Instant::now(),
                    },
                );
                Ok(WebhookIdempotencyClaim::Claimed)
            }
            Some(InMemoryWebhookStatus::Processed) => Ok(WebhookIdempotencyClaim::AlreadyProcessed),
            Some(InMemoryWebhookStatus::Pending) => {
                statuses.insert(
                    provider_event_id.to_owned(),
                    InMemoryWebhookStatus::Processing {
                        started_at: Instant::now(),
                    },
                );
                Ok(WebhookIdempotencyClaim::Claimed)
            }
            Some(InMemoryWebhookStatus::Processing { started_at }) => {
                if started_at.elapsed() >= stale_timeout {
                    statuses.insert(
                        provider_event_id.to_owned(),
                        InMemoryWebhookStatus::Processing {
                            started_at: Instant::now(),
                        },
                    );
                    Ok(WebhookIdempotencyClaim::Claimed)
                } else {
                    Ok(WebhookIdempotencyClaim::InProgress)
                }
            }
        }
    }

    async fn mark_processed(
        &self,
        provider_event_id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.statuses.lock().unwrap().insert(
            provider_event_id.to_owned(),
            InMemoryWebhookStatus::Processed,
        );
        Ok(())
    }

    async fn mark_pending(
        &self,
        provider_event_id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.statuses
            .lock()
            .unwrap()
            .insert(provider_event_id.to_owned(), InMemoryWebhookStatus::Pending);
        Ok(())
    }
}
