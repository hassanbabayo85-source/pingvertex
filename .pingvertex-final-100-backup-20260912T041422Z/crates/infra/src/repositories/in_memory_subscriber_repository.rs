use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use application::ports::SubscriberRepository;
use async_trait::async_trait;
use domain::{
    Subscriber,
    shared::ids::{MonitorId, SubscriberId},
};

#[derive(Debug, Clone, Default)]
pub struct InMemorySubscriberRepository {
    subscribers: Arc<RwLock<HashMap<SubscriberId, Subscriber>>>,
}

impl InMemorySubscriberRepository {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl SubscriberRepository for InMemorySubscriberRepository {
    async fn save(
        &self,
        subscriber: &Subscriber,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.subscribers
            .write()
            .unwrap()
            .insert(subscriber.id, subscriber.clone());

        Ok(())
    }

    async fn delete(
        &self,
        id: SubscriberId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.subscribers.write().unwrap().remove(&id).is_some())
    }

    async fn find_by_id(
        &self,
        id: SubscriberId,
    ) -> Result<Option<Subscriber>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.subscribers.read().unwrap().get(&id).cloned())
    }

    async fn list_by_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> Result<Vec<Subscriber>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .subscribers
            .read()
            .unwrap()
            .values()
            .filter(|s| s.monitor_id == monitor_id)
            .cloned()
            .collect())
    }
}
