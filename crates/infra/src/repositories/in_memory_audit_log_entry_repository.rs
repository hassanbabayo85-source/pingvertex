use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use application::ports::AuditLogEntryRepository;
use async_trait::async_trait;
use domain::{
    AuditLogEntry,
    shared::ids::{TenantId, UserId},
};

#[derive(Debug, Clone, Default)]
pub struct InMemoryAuditLogEntryRepository {
    entries: Arc<Mutex<Vec<AuditLogEntry>>>,
}

impl InMemoryAuditLogEntryRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AuditLogEntryRepository for InMemoryAuditLogEntryRepository {
    async fn create(&self, entry: &AuditLogEntry) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.entries.lock().unwrap().push(entry.clone());
        Ok(())
    }

    async fn list_by_tenant(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<AuditLogEntry>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == Some(tenant_id))
            .cloned()
            .collect())
    }

    async fn list_by_actor(
        &self,
        actor_user_id: UserId,
    ) -> Result<Vec<AuditLogEntry>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.actor_user_id == actor_user_id)
            .cloned()
            .collect())
    }
}
