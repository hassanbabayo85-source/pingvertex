use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::FinancialAuditLogEntry;
use std::{error::Error, sync::Arc};
use uuid::Uuid;

pub type FinancialAuditRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Default)]
pub struct FinancialAuditLogFilter {
    pub action: Option<String>,
    pub resource: Option<String>,
    pub resource_id: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Copy)]
pub struct FinancialAuditLogCursor {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}
#[derive(Debug, Clone)]
pub struct FinancialAuditLogPage {
    pub entries: Vec<FinancialAuditLogEntry>,
    pub has_more: bool,
}

#[async_trait]
pub trait FinancialAuditLogRepository: Send + Sync {
    async fn create(&self, entry: &FinancialAuditLogEntry) -> FinancialAuditRepoResult<()>;
    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
    ) -> FinancialAuditRepoResult<Vec<FinancialAuditLogEntry>>;

    async fn list_page(
        &self,
        tenant_id: Uuid,
        filter: &FinancialAuditLogFilter,
        cursor: Option<FinancialAuditLogCursor>,
        limit: u32,
    ) -> FinancialAuditRepoResult<FinancialAuditLogPage> {
        let limit = limit.clamp(1, 100) as usize;
        let mut rows = self.list_by_tenant(tenant_id).await?;
        rows.retain(|entry| {
            filter.action.as_deref().is_none_or(|v| entry.action == v)
                && filter
                    .resource
                    .as_deref()
                    .is_none_or(|v| entry.resource == v)
                && filter
                    .resource_id
                    .as_deref()
                    .is_none_or(|v| entry.resource_id.as_deref() == Some(v))
                && filter.from.is_none_or(|v| entry.created_at >= v)
                && filter.to.is_none_or(|v| entry.created_at < v)
                && cursor.is_none_or(|c| {
                    entry.created_at < c.created_at
                        || (entry.created_at == c.created_at && entry.id < c.id)
                })
        });
        rows.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        Ok(FinancialAuditLogPage {
            entries: rows,
            has_more,
        })
    }
}

#[async_trait]
impl<T> FinancialAuditLogRepository for Arc<T>
where
    T: FinancialAuditLogRepository + Send + Sync + ?Sized,
{
    async fn create(&self, e: &FinancialAuditLogEntry) -> FinancialAuditRepoResult<()> {
        (**self).create(e).await
    }
    async fn list_by_tenant(
        &self,
        t: Uuid,
    ) -> FinancialAuditRepoResult<Vec<FinancialAuditLogEntry>> {
        (**self).list_by_tenant(t).await
    }
    async fn list_page(
        &self,
        tenant_id: Uuid,
        filter: &FinancialAuditLogFilter,
        cursor: Option<FinancialAuditLogCursor>,
        limit: u32,
    ) -> FinancialAuditRepoResult<FinancialAuditLogPage> {
        (**self).list_page(tenant_id, filter, cursor, limit).await
    }
}
