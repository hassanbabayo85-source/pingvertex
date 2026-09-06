use application::ports::{UsageEventRepoResult, UsageEventRepository};
use async_trait::async_trait;
use domain::UsageEvent;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct InMemoryUsageEventRepository {
    rows: Arc<Mutex<HashMap<(Uuid, String), UsageEvent>>>,
}
#[async_trait]
impl UsageEventRepository for InMemoryUsageEventRepository {
    async fn find_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> UsageEventRepoResult<Option<UsageEvent>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .get(&(tenant_id, request_id.to_owned()))
            .cloned())
    }
    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent> {
        let mut rows = self.rows.lock().unwrap();
        let key = (event.tenant_id, event.request_id.clone());
        if let Some(existing) = rows.get(&key) {
            return Ok(existing.clone());
        }
        rows.insert(key, event.clone());
        Ok(event.clone())
    }
}
