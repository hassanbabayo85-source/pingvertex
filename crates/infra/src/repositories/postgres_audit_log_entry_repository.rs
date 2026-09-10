use std::error::Error;

use application::ports::{AuditLogCursor, AuditLogEntryRepository, AuditLogFilter, AuditLogPage};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    AuditLogEntry,
    enums::AuditAction,
    shared::ids::{AuditLogEntryId, TenantId, UserId},
};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresAuditLogEntryRepository {
    pool: PgPool,
}

impl PostgresAuditLogEntryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn action_to_db(action: &AuditAction) -> String {
    format!("{:?}", action)
}

fn action_from_db(value: &str) -> RepoResult<AuditAction> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|_| format!("unknown audit_log.action '{value}' in database").into())
}

#[derive(Debug, Clone, FromRow)]
struct AuditLogEntryRow {
    id: Uuid,
    actor_user_id: Uuid,
    tenant_id: Option<Uuid>,
    action: String,
    target_id: String,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
}

impl AuditLogEntryRow {
    fn from_domain(entry: &AuditLogEntry) -> Self {
        Self {
            id: entry.id.0,
            actor_user_id: entry.actor_user_id.0,
            tenant_id: entry.tenant_id.map(|t| t.0),
            action: action_to_db(&entry.action),
            target_id: entry.target_id.clone(),
            metadata: entry.metadata.clone(),
            created_at: entry.created_at,
        }
    }

    fn into_domain(self) -> RepoResult<AuditLogEntry> {
        Ok(AuditLogEntry {
            id: AuditLogEntryId(self.id),
            actor_user_id: UserId(self.actor_user_id),
            tenant_id: self.tenant_id.map(TenantId),
            action: action_from_db(&self.action)?,
            target_id: self.target_id,
            metadata: self.metadata,
            created_at: self.created_at,
        })
    }
}

#[async_trait]
impl AuditLogEntryRepository for PostgresAuditLogEntryRepository {
    async fn create(&self, entry: &AuditLogEntry) -> RepoResult<()> {
        let row = AuditLogEntryRow::from_domain(entry);
        sqlx::query(
            "INSERT INTO audit_log \
             (id, actor_user_id, tenant_id, action, target_id, metadata, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(row.id)
        .bind(row.actor_user_id)
        .bind(row.tenant_id)
        .bind(row.action)
        .bind(row.target_id)
        .bind(row.metadata)
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<AuditLogEntry>> {
        let rows = sqlx::query_as::<_, AuditLogEntryRow>(
            "SELECT id, actor_user_id, tenant_id, action, target_id, metadata, created_at \
             FROM audit_log WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(AuditLogEntryRow::into_domain)
            .collect()
    }

    async fn list_all(&self, limit: u32) -> RepoResult<Vec<AuditLogEntry>> {
        let rows=sqlx::query_as::<_,AuditLogEntryRow>("SELECT id, actor_user_id, tenant_id, action, target_id, metadata, created_at FROM audit_log ORDER BY created_at DESC LIMIT $1").bind(limit.clamp(1,1000) as i64).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(AuditLogEntryRow::into_domain)
            .collect()
    }

    async fn list_page(
        &self,
        filter: &AuditLogFilter,
        cursor: Option<AuditLogCursor>,
        limit: u32,
    ) -> RepoResult<AuditLogPage> {
        let limit = limit.clamp(1, 100) as i64;
        let mut q = QueryBuilder::<Postgres>::new(
            "SELECT id, actor_user_id, tenant_id, action, target_id, metadata, created_at FROM audit_log WHERE 1=1",
        );
        if let Some(actor) = filter.actor_user_id {
            q.push(" AND actor_user_id = ").push_bind(actor.0);
        }
        if let Some(tenant) = filter.tenant_id {
            q.push(" AND tenant_id = ").push_bind(tenant.0);
        }
        if let Some(action) = filter.action.as_deref() {
            q.push(" AND action = ").push_bind(action);
        }
        if let Some(target) = filter.target_id.as_deref() {
            q.push(" AND target_id = ").push_bind(target);
        }
        if let Some(from) = filter.from {
            q.push(" AND created_at >= ").push_bind(from);
        }
        if let Some(to) = filter.to {
            q.push(" AND created_at < ").push_bind(to);
        }
        if let Some(c) = cursor {
            q.push(" AND (created_at < ")
                .push_bind(c.created_at)
                .push(" OR (created_at = ")
                .push_bind(c.created_at)
                .push(" AND id < ")
                .push_bind(c.id)
                .push("))");
        }
        q.push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind(limit + 1);
        let rows = q
            .build_query_as::<AuditLogEntryRow>()
            .fetch_all(&self.pool)
            .await?;
        let has_more = rows.len() > limit as usize;
        let entries = rows
            .into_iter()
            .take(limit as usize)
            .map(AuditLogEntryRow::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;
        Ok(AuditLogPage { entries, has_more })
    }

    async fn list_by_actor(&self, actor_user_id: UserId) -> RepoResult<Vec<AuditLogEntry>> {
        let rows = sqlx::query_as::<_, AuditLogEntryRow>(
            "SELECT id, actor_user_id, tenant_id, action, target_id, metadata, created_at \
             FROM audit_log WHERE actor_user_id = $1 ORDER BY created_at DESC",
        )
        .bind(actor_user_id.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(AuditLogEntryRow::into_domain)
            .collect()
    }
}
