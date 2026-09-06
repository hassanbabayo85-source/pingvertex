use application::ports::{FinancialAuditLogRepository, FinancialAuditRepoResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::FinancialAuditLogEntry;
use sqlx::{FromRow, PgPool};
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
}
