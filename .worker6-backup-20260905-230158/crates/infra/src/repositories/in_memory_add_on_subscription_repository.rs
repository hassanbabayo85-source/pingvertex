use application::ports::AddOnSubscriptionRepository;
use async_trait::async_trait;
use domain::shared::ids::TenantId;
use domain::{AddOnStatus, AddOnSubscription, AddOnSubscriptionId, AddOnType};
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default)]
pub struct InMemoryAddOnSubscriptionRepository {
    items: Arc<Mutex<HashMap<AddOnSubscriptionId, AddOnSubscription>>>,
}
impl InMemoryAddOnSubscriptionRepository {
    pub fn new() -> Self {
        Self::default()
    }
}
type R<T> = Result<T, Box<dyn Error + Send + Sync>>;
#[async_trait]
impl AddOnSubscriptionRepository for InMemoryAddOnSubscriptionRepository {
    async fn create(&self, a: &AddOnSubscription) -> R<()> {
        self.items.lock().unwrap().insert(a.id, a.clone());
        Ok(())
    }
    async fn find_by_id(&self, id: AddOnSubscriptionId) -> R<Option<AddOnSubscription>> {
        Ok(self.items.lock().unwrap().get(&id).cloned())
    }
    async fn list_active_by_tenant(&self, t: TenantId) -> R<Vec<AddOnSubscription>> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .filter(|a| a.tenant_id == t && a.status == AddOnStatus::Active)
            .cloned()
            .collect())
    }
    async fn list_by_tenant_and_type(
        &self,
        t: TenantId,
        k: AddOnType,
    ) -> R<Vec<AddOnSubscription>> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .filter(|a| a.tenant_id == t && a.addon_type == k)
            .cloned()
            .collect())
    }
}
