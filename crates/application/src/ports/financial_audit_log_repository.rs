use async_trait::async_trait;
use domain::FinancialAuditLogEntry;
use std::{error::Error, sync::Arc};
use uuid::Uuid;

pub type FinancialAuditRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait FinancialAuditLogRepository: Send + Sync {
    async fn create(&self, entry: &FinancialAuditLogEntry) -> FinancialAuditRepoResult<()>;
    async fn list_by_tenant(&self, tenant_id: Uuid) -> FinancialAuditRepoResult<Vec<FinancialAuditLogEntry>>;
}

#[async_trait]
impl<T> FinancialAuditLogRepository for Arc<T>
where T: FinancialAuditLogRepository + Send + Sync + ?Sized {
    async fn create(&self, e: &FinancialAuditLogEntry) -> FinancialAuditRepoResult<()> { (**self).create(e).await }
    async fn list_by_tenant(&self, t: Uuid) -> FinancialAuditRepoResult<Vec<FinancialAuditLogEntry>> { (**self).list_by_tenant(t).await }
}

