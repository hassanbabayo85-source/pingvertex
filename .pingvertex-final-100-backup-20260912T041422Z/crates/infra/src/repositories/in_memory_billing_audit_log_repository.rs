use std::error::Error;
use std::sync::{Arc, Mutex};

use application::ports::BillingAuditLogRepository;
use async_trait::async_trait;
use domain::BillingAuditLogEntry;

#[derive(Debug, Clone, Default)]
pub struct InMemoryBillingAuditLogRepository {
    entries: Arc<Mutex<Vec<BillingAuditLogEntry>>>,
}

impl InMemoryBillingAuditLogRepository {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn entries(&self) -> Vec<BillingAuditLogEntry> {
        self.entries.lock().unwrap().clone()
    }
}

#[async_trait]
impl BillingAuditLogRepository for InMemoryBillingAuditLogRepository {
    async fn create(
        &self,
        entry: &BillingAuditLogEntry,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.entries.lock().unwrap().push(entry.clone());
        Ok(())
    }
}
