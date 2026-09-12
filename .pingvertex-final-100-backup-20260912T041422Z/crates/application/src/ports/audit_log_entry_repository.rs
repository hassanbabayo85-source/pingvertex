use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::{error::Error, sync::Arc};
use uuid::Uuid;

use domain::{
    AuditLogEntry,
    shared::ids::{TenantId, UserId},
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Default)]
pub struct AuditLogFilter {
    pub actor_user_id: Option<UserId>,
    pub tenant_id: Option<TenantId>,
    pub action: Option<String>,
    pub target_id: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy)]
pub struct AuditLogCursor {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}

#[derive(Debug, Clone)]
pub struct AuditLogPage {
    pub entries: Vec<AuditLogEntry>,
    pub has_more: bool,
}

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

    async fn list_page(
        &self,
        filter: &AuditLogFilter,
        cursor: Option<AuditLogCursor>,
        limit: u32,
    ) -> RepoResult<AuditLogPage> {
        let limit = limit.clamp(1, 100);
        let mut rows = if let Some(tenant_id) = filter.tenant_id {
            self.list_by_tenant(tenant_id).await?
        } else if let Some(actor) = filter.actor_user_id {
            self.list_by_actor(actor).await?
        } else {
            self.list_all(limit + 1).await?
        };
        rows.retain(|entry| {
            filter
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
        rows.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.0.cmp(&a.id.0))
        });
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        Ok(AuditLogPage {
            entries: rows,
            has_more,
        })
    }
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

    async fn list_page(
        &self,
        filter: &AuditLogFilter,
        cursor: Option<AuditLogCursor>,
        limit: u32,
    ) -> RepoResult<AuditLogPage> {
        (**self).list_page(filter, cursor, limit).await
    }
}
