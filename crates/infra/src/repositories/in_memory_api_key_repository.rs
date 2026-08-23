use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use application::ports::ApiKeyRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    entities::ApiKey,
    shared::ids::{ApiKeyId, TenantId},
};

#[derive(Debug, Clone, Default)]
pub struct InMemoryApiKeyRepository {
    keys: Arc<Mutex<Vec<ApiKey>>>,
}

impl InMemoryApiKeyRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ApiKeyRepository for InMemoryApiKeyRepository {
    async fn create(&self, api_key: &ApiKey) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.keys.lock().unwrap().push(api_key.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: ApiKeyId,
    ) -> Result<Option<ApiKey>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .keys
            .lock()
            .unwrap()
            .iter()
            .find(|k| k.id == id)
            .cloned())
    }

    async fn find_by_prefix(
        &self,
        key_prefix: &str,
    ) -> Result<Option<ApiKey>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .keys
            .lock()
            .unwrap()
            .iter()
            .find(|k| k.key_prefix == key_prefix)
            .cloned())
    }

    async fn list_by_tenant(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<ApiKey>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .keys
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn delete(&self, id: ApiKeyId) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let mut keys = self.keys.lock().unwrap();
        let len_before = keys.len();
        keys.retain(|k| k.id != id);
        Ok(keys.len() != len_before)
    }

    async fn touch_last_used(
        &self,
        id: ApiKeyId,
        at: DateTime<Utc>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(key) = self.keys.lock().unwrap().iter_mut().find(|k| k.id == id) {
            key.last_used_at = Some(at);
        }
        Ok(())
    }
}
