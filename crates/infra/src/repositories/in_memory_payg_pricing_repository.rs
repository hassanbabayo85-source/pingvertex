use application::ports::{PAYGPricingRepository, PaygPricingRepoResult};
use async_trait::async_trait;
use domain::PAYGPricing;
use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct InMemoryPAYGPricingRepository {
    rows: Arc<RwLock<Vec<PAYGPricing>>>,
}
impl InMemoryPAYGPricingRepository {
    pub fn new(rows: Vec<PAYGPricing>) -> Self {
        Self {
            rows: Arc::new(RwLock::new(rows)),
        }
    }
    pub fn insert(&self, row: PAYGPricing) {
        self.rows.write().unwrap().push(row);
    }
}
#[async_trait]
impl PAYGPricingRepository for InMemoryPAYGPricingRepository {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let now = chrono::Utc::now();
        let row = self
            .rows
            .read()
            .unwrap()
            .iter()
            .filter(|p| p.operation == operation && p.active && p.effective_from <= now)
            .filter(|p| p.effective_until.map(|v| v > now).unwrap_or(true))
            .max_by_key(|p| p.version)
            .cloned();
        Ok(row)
    }
    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
        let now=chrono::Utc::now();
        Ok(self.rows.read().unwrap().iter().filter(|p|p.active && p.effective_from<=now && p.effective_until.map(|v|v>now).unwrap_or(true)).cloned().collect())
    }
}
