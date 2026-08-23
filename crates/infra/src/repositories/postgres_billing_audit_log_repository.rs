use std::error::Error;

use application::ports::BillingAuditLogRepository;
use async_trait::async_trait;
use domain::BillingAuditLogEntry;
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct PostgresBillingAuditLogRepository {
    pool: PgPool,
}

impl PostgresBillingAuditLogRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BillingAuditLogRepository for PostgresBillingAuditLogRepository {
    async fn create(
        &self,
        entry: &BillingAuditLogEntry,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query(
            "INSERT INTO billing_audit_log \
             (id, provider_event_id, event_type, tenant_id, subscription_id, payload, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(entry.id.0)
        .bind(&entry.provider_event_id)
        .bind(entry.event_type.as_str())
        .bind(entry.tenant_id.0)
        .bind(entry.subscription_id.map(|s| s.0))
        .bind(&entry.payload)
        .bind(entry.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
