use application::ports::{
    FinancialAuditLogCursor, FinancialAuditLogFilter, FinancialAuditLogPage,
    FinancialAuditLogRepository, FinancialAuditRepoResult,
};
use async_trait::async_trait;
use domain::FinancialAuditLogEntry;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct InMemoryFinancialAuditLogRepository {
    entries: Arc<Mutex<Vec<FinancialAuditLogEntry>>>,
}
impl InMemoryFinancialAuditLogRepository {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn entries(&self) -> Vec<FinancialAuditLogEntry> {
        self.entries.lock().unwrap().clone()
    }
}
#[async_trait]
impl FinancialAuditLogRepository for InMemoryFinancialAuditLogRepository {
    async fn create(&self, e: &FinancialAuditLogEntry) -> FinancialAuditRepoResult<()> {
        self.entries.lock().unwrap().push(e.clone());
        Ok(())
    }
    async fn list_by_tenant(
        &self,
        t: Uuid,
    ) -> FinancialAuditRepoResult<Vec<FinancialAuditLogEntry>> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.tenant_id == Some(t))
            .cloned()
            .collect())
    }

    async fn list_page_admin(
        &self,
        tenant_id: Option<Uuid>,
        filter: &FinancialAuditLogFilter,
        cursor: Option<FinancialAuditLogCursor>,
        limit: u32,
    ) -> FinancialAuditRepoResult<FinancialAuditLogPage> {
        let limit = limit.clamp(1, 100) as usize;
        let mut rows: Vec<FinancialAuditLogEntry> = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| tenant_id.is_none_or(|t| e.tenant_id == Some(t)))
            .filter(|e| filter.action.as_deref().is_none_or(|v| e.action == v))
            .filter(|e| filter.resource.as_deref().is_none_or(|v| e.resource == v))
            .filter(|e| {
                filter
                    .resource_id
                    .as_deref()
                    .is_none_or(|v| e.resource_id.as_deref() == Some(v))
            })
            .filter(|e| filter.from.is_none_or(|v| e.created_at >= v))
            .filter(|e| filter.to.is_none_or(|v| e.created_at < v))
            .filter(|e| {
                cursor.is_none_or(|c| {
                    e.created_at < c.created_at
                        || (e.created_at == c.created_at && e.id < c.id)
                })
            })
            .cloned()
            .collect();
        rows.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        Ok(FinancialAuditLogPage { entries: rows, has_more })
    }
}
