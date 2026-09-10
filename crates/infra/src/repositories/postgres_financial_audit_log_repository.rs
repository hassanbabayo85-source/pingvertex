use application::ports::{
    FinancialAuditLogCursor, FinancialAuditLogFilter, FinancialAuditLogPage,
    FinancialAuditLogRepository, FinancialAuditRepoResult,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::FinancialAuditLogEntry;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

pub struct PostgresFinancialAuditLogRepository {
    pool: PgPool,
}
impl PostgresFinancialAuditLogRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct Row {
    id: Uuid,
    actor: String,
    tenant_id: Option<Uuid>,
    action: String,
    resource: String,
    resource_id: Option<String>,
    created_at: DateTime<Utc>,
    metadata: serde_json::Value,
}

#[async_trait]
impl FinancialAuditLogRepository for PostgresFinancialAuditLogRepository {
    async fn create(&self, e: &FinancialAuditLogEntry) -> FinancialAuditRepoResult<()> {
        sqlx::query("INSERT INTO financial_audit_log(id,actor,tenant_id,action,resource,resource_id,created_at,metadata) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(e.id).bind(&e.actor).bind(e.tenant_id).bind(&e.action).bind(&e.resource)
            .bind(&e.resource_id).bind(e.created_at).bind(&e.metadata).execute(&self.pool).await?;
        Ok(())
    }
    async fn list_page(
        &self,
        tenant_id: Uuid,
        filter: &FinancialAuditLogFilter,
        cursor: Option<FinancialAuditLogCursor>,
        limit: u32,
    ) -> FinancialAuditRepoResult<FinancialAuditLogPage> {
        let limit = limit.clamp(1, 100) as i64;
        let mut q = QueryBuilder::<Postgres>::new(
            "SELECT id,actor,tenant_id,action,resource,resource_id,created_at,metadata FROM financial_audit_log WHERE tenant_id=",
        );
        q.push_bind(tenant_id);
        if let Some(action) = filter.action.as_deref() {
            q.push(" AND action=").push_bind(action);
        }
        if let Some(resource) = filter.resource.as_deref() {
            q.push(" AND resource=").push_bind(resource);
        }
        if let Some(resource_id) = filter.resource_id.as_deref() {
            q.push(" AND resource_id=").push_bind(resource_id);
        }
        if let Some(from) = filter.from {
            q.push(" AND created_at>=").push_bind(from);
        }
        if let Some(to) = filter.to {
            q.push(" AND created_at<").push_bind(to);
        }
        if let Some(c) = cursor {
            q.push(" AND (created_at<")
                .push_bind(c.created_at)
                .push(" OR (created_at=")
                .push_bind(c.created_at)
                .push(" AND id<")
                .push_bind(c.id)
                .push("))");
        }
        q.push(" ORDER BY created_at DESC,id DESC LIMIT ")
            .push_bind(limit + 1);
        let rows = q.build_query_as::<Row>().fetch_all(&self.pool).await?;
        let has_more = rows.len() > limit as usize;
        let entries = rows
            .into_iter()
            .take(limit as usize)
            .map(|r| FinancialAuditLogEntry {
                id: r.id,
                actor: r.actor,
                tenant_id: r.tenant_id,
                action: r.action,
                resource: r.resource,
                resource_id: r.resource_id,
                created_at: r.created_at,
                metadata: r.metadata,
            })
            .collect();
        Ok(FinancialAuditLogPage { entries, has_more })
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
    ) -> FinancialAuditRepoResult<Vec<FinancialAuditLogEntry>> {
        let rows = sqlx::query_as::<_,Row>("SELECT id,actor,tenant_id,action,resource,resource_id,created_at,metadata FROM financial_audit_log WHERE tenant_id=$1 ORDER BY created_at DESC LIMIT 500")
            .bind(tenant_id).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| FinancialAuditLogEntry {
                id: r.id,
                actor: r.actor,
                tenant_id: r.tenant_id,
                action: r.action,
                resource: r.resource,
                resource_id: r.resource_id,
                created_at: r.created_at,
                metadata: r.metadata,
            })
            .collect())
    }

    async fn list_page_admin(
        &self,
        tenant_id: Option<Uuid>,
        filter: &FinancialAuditLogFilter,
        cursor: Option<FinancialAuditLogCursor>,
        limit: u32,
    ) -> FinancialAuditRepoResult<FinancialAuditLogPage> {
        let limit = limit.clamp(1, 100) as i64;
        let mut q = QueryBuilder::<Postgres>::new(
            "SELECT id,actor,tenant_id,action,resource,resource_id,created_at,metadata FROM financial_audit_log WHERE 1=1",
        );
        if let Some(t) = tenant_id {
            q.push(" AND tenant_id=").push_bind(t);
        }
        if let Some(action) = filter.action.as_deref() {
            q.push(" AND action=").push_bind(action);
        }
        if let Some(resource) = filter.resource.as_deref() {
            q.push(" AND resource=").push_bind(resource);
        }
        if let Some(resource_id) = filter.resource_id.as_deref() {
            q.push(" AND resource_id=").push_bind(resource_id);
        }
        if let Some(from) = filter.from {
            q.push(" AND created_at>=").push_bind(from);
        }
        if let Some(to) = filter.to {
            q.push(" AND created_at<").push_bind(to);
        }
        if let Some(c) = cursor {
            q.push(" AND (created_at<")
                .push_bind(c.created_at)
                .push(" OR (created_at=")
                .push_bind(c.created_at)
                .push(" AND id<")
                .push_bind(c.id)
                .push("))");
        }
        q.push(" ORDER BY created_at DESC,id DESC LIMIT ")
            .push_bind(limit + 1);
        let rows = q.build_query_as::<Row>().fetch_all(&self.pool).await?;
        let has_more = rows.len() > limit as usize;
        let entries = rows
            .into_iter()
            .take(limit as usize)
            .map(|r| FinancialAuditLogEntry {
                id: r.id,
                actor: r.actor,
                tenant_id: r.tenant_id,
                action: r.action,
                resource: r.resource,
                resource_id: r.resource_id,
                created_at: r.created_at,
                metadata: r.metadata,
            })
            .collect();
        Ok(FinancialAuditLogPage { entries, has_more })
    }
}
