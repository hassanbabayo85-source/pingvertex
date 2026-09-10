use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use application::ports::{AuditLogCursor, AuditLogEntryRepository, AuditLogFilter, AuditLogPage};
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

    async fn list_all(
        &self,
        limit: u32,
    ) -> Result<Vec<AuditLogEntry>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .rev()
            .take(limit.clamp(1, 1000) as usize)
            .cloned()
            .collect())
    }

    async fn list_page(
        &self,
        filter: &AuditLogFilter,
        cursor: Option<AuditLogCursor>,
        limit: u32,
    ) -> Result<AuditLogPage, Box<dyn Error + Send + Sync>> {
        let mut entries = self.entries.lock().unwrap().clone();
        entries.retain(|entry| {
            filter
                .tenant_id
                .is_none_or(|tenant| entry.tenant_id == Some(tenant))
                && filter
                    .actor_user_id
                    .is_none_or(|actor| entry.actor_user_id == actor)
                && filter
                    .action
                    .as_deref()
                    .is_none_or(|action| format!("{:?}", entry.action) == action)
                && filter
                    .target_id
                    .as_deref()
                    .is_none_or(|target| entry.target_id == target)
                && filter.from.is_none_or(|from| entry.created_at >= from)
                && filter.to.is_none_or(|to| entry.created_at < to)
                && cursor.is_none_or(|c| {
                    entry.created_at < c.created_at
                        || (entry.created_at == c.created_at && entry.id.0 < c.id)
                })
        });
        entries.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.0.cmp(&a.id.0))
        });
        let limit = limit.clamp(1, 100) as usize;
        let has_more = entries.len() > limit;
        entries.truncate(limit);
        Ok(AuditLogPage { entries, has_more })
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
