use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use application::ports::SubscriptionRepository;
use async_trait::async_trait;
use domain::Subscription;
use domain::shared::ids::TenantId;

/// Test-only in-memory stand-in for `SubscriptionRepository`.
///
/// Mirrors `InMemoryTenantRepository`'s style: a plain
/// `Mutex<HashMap<..>>` behind an `Arc` so cloned handles share state,
/// with no persistence beyond the life of the test.
#[derive(Debug, Clone, Default)]
pub struct InMemorySubscriptionRepository {
    subscriptions: Arc<Mutex<HashMap<TenantId, Subscription>>>,
}

impl InMemorySubscriptionRepository {
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl SubscriptionRepository for InMemorySubscriptionRepository {
    async fn create(
        &self,
        subscription: &Subscription,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.subscriptions
            .lock()
            .unwrap()
            .insert(subscription.tenant_id, subscription.clone());
        Ok(())
    }

    async fn find_by_tenant_id(
        &self,
        tenant_id: TenantId,
    ) -> Result<Option<Subscription>, Box<dyn Error + Send + Sync>> {
        Ok(self.subscriptions.lock().unwrap().get(&tenant_id).cloned())
    }

    async fn update(
        &self,
        subscription: &Subscription,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.subscriptions
            .lock()
            .unwrap()
            .insert(subscription.tenant_id, subscription.clone());
        Ok(())
    }
}
