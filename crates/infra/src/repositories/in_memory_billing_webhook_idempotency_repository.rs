use std::collections::HashSet;
use std::error::Error;
use std::sync::{Arc, Mutex};

use application::ports::BillingWebhookIdempotencyRepository;
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct InMemoryBillingWebhookIdempotencyRepository {
    keys: Arc<Mutex<HashSet<String>>>,
}

impl InMemoryBillingWebhookIdempotencyRepository {
    pub fn new() -> Self {
        Self {
            keys: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

#[async_trait]
impl BillingWebhookIdempotencyRepository for InMemoryBillingWebhookIdempotencyRepository {
    async fn try_insert(
        &self,
        provider_event_id: &str,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(self
            .keys
            .lock()
            .unwrap()
            .insert(provider_event_id.to_owned()))
    }
}
