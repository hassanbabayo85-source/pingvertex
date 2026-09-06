use application::ports::{FinancialAuditLogRepository, FinancialAuditRepoResult};
use async_trait::async_trait;
use domain::FinancialAuditLogEntry;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct InMemoryFinancialAuditLogRepository { entries: Arc<Mutex<Vec<FinancialAuditLogEntry>>> }
impl InMemoryFinancialAuditLogRepository { pub fn new() -> Self { Self::default() } pub fn entries(&self) -> Vec<FinancialAuditLogEntry> { self.entries.lock().unwrap().clone() } }
#[async_trait]
impl FinancialAuditLogRepository for InMemoryFinancialAuditLogRepository {
 async fn create(&self,e:&FinancialAuditLogEntry)->FinancialAuditRepoResult<()> { self.entries.lock().unwrap().push(e.clone()); Ok(()) }
 async fn list_by_tenant(&self,t:Uuid)->FinancialAuditRepoResult<Vec<FinancialAuditLogEntry>> { Ok(self.entries.lock().unwrap().iter().filter(|e|e.tenant_id==Some(t)).cloned().collect()) }
}

