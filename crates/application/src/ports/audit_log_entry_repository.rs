use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::{
    AuditLogEntry,
    shared::ids::{TenantId, UserId},
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Append-only persistence port for audit log entries.
///
/// Deliberately has NO `update` and NO `delete` methods. Audit logs must
/// be append-only. Because this project currently connects as the
/// PostgreSQL `postgres` superuser with no distinct least-privileged
/// application DB role (see `docker-compose.yml`), a DB-level
/// `REVOKE UPDATE, DELETE` would be unenforceable and has therefore NOT
/// been added in `migrations/0029_create_audit_log.sql`. This trait is
/// the primary guard: consumers cannot update or delete an audit log
/// entry through this port.
///
/// If a distinct least-privileged DB role is introduced later, add
/// `REVOKE UPDATE, DELETE ON audit_log` to that migration and keep this
/// trait append-only.
#[async_trait]
pub trait AuditLogEntryRepository: Send + Sync {
    async fn create(&self, entry: &AuditLogEntry) -> RepoResult<()>;

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<AuditLogEntry>>;

    async fn list_by_actor(&self, actor_user_id: UserId) -> RepoResult<Vec<AuditLogEntry>>;
    async fn list_all(&self, _limit: u32) -> RepoResult<Vec<AuditLogEntry>> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl<T> AuditLogEntryRepository for Arc<T>
where
    T: AuditLogEntryRepository + Send + Sync + ?Sized,
{
    async fn create(&self, entry: &AuditLogEntry) -> RepoResult<()> {
        (**self).create(entry).await
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<AuditLogEntry>> {
        (**self).list_by_tenant(tenant_id).await
    }

    async fn list_by_actor(&self, actor_user_id: UserId) -> RepoResult<Vec<AuditLogEntry>> {
        (**self).list_by_actor(actor_user_id).await
    }
    async fn list_all(&self, limit: u32) -> RepoResult<Vec<AuditLogEntry>> {
        (**self).list_all(limit).await
    }
}
