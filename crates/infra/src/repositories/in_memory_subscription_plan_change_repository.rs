use application::ports::*;
use async_trait::async_trait;
use domain::SubscriptionPlanChange;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
#[derive(Clone, Default)]
pub struct InMemorySubscriptionPlanChangeRepository {
    pub items: Arc<Mutex<Vec<SubscriptionPlanChange>>>,
}
impl InMemorySubscriptionPlanChangeRepository {
    pub fn new() -> Self {
        Self::default()
    }
}
#[async_trait]
impl SubscriptionPlanChangeRepository for InMemorySubscriptionPlanChangeRepository {
    async fn create(&self, c: &SubscriptionPlanChange) -> PlanChangeRepoResult<()> {
        self.items.lock().unwrap().push(c.clone());
        Ok(())
    }
    async fn update(&self, c: &SubscriptionPlanChange) -> PlanChangeRepoResult<()> {
        let mut v = self.items.lock().unwrap();
        if let Some(x) = v
            .iter_mut()
            .find(|x| x.id == c.id && x.tenant_id == c.tenant_id)
        {
            *x = c.clone();
            Ok(())
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "plan change not found").into())
        }
    }
    async fn find_by_id(
        &self,
        id: Uuid,
        t: Uuid,
    ) -> PlanChangeRepoResult<Option<SubscriptionPlanChange>> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .iter()
            .find(|x| x.id == id && x.tenant_id == t)
            .cloned())
    }
}
