use std::error::Error;

use application::ports::EscalationPolicyRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::{EscalationPolicyId, MonitorId, TenantId, UserId};
use domain::{EscalationPolicy, EscalationStep};
use serde_json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresEscalationPolicyRepository {
    pool: PgPool,
}

impl PostgresEscalationPolicyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct EscalationPolicyRow {
    id: Uuid,
    user_id: Uuid,
    tenant_id: Uuid,
    monitor_id: Option<Uuid>,
    steps: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl EscalationPolicyRow {
    fn from_domain(policy: &EscalationPolicy) -> Self {
        let steps_json = serde_json::to_value(&policy.steps).expect("steps serializable");
        Self {
            id: policy.id.0,
            user_id: policy.user_id.0,
            tenant_id: policy.tenant_id.0,
            monitor_id: policy.monitor_id.map(|m| m.0),
            steps: steps_json,
            created_at: policy.created_at,
            updated_at: policy.updated_at,
        }
    }

    fn into_domain(self) -> RepoResult<EscalationPolicy> {
        let steps: Vec<EscalationStep> = serde_json::from_value(self.steps)
            .map_err(|e| format!("invalid escalation steps JSON in database: {e}"))?;

        Ok(EscalationPolicy {
            id: EscalationPolicyId(self.id),
            user_id: UserId(self.user_id),
            tenant_id: TenantId(self.tenant_id),
            monitor_id: self.monitor_id.map(MonitorId),
            steps,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

const SELECT_COLUMNS: &str = "id, user_id, tenant_id, monitor_id, steps, created_at, updated_at";

#[async_trait]
impl EscalationPolicyRepository for PostgresEscalationPolicyRepository {
    async fn create(&self, policy: &EscalationPolicy) -> RepoResult<()> {
        let row = EscalationPolicyRow::from_domain(policy);

        sqlx::query(
            r#"
            INSERT INTO escalation_policies (
                id, user_id, tenant_id, monitor_id, steps, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.tenant_id)
        .bind(row.monitor_id)
        .bind(row.steps)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update(&self, policy: &EscalationPolicy) -> RepoResult<()> {
        let row = EscalationPolicyRow::from_domain(policy);

        sqlx::query(
            r#"
            UPDATE escalation_policies
            SET
                user_id = $2,
                monitor_id = $3,
                steps = $4,
                created_at = $5,
                updated_at = $6
            WHERE id = $1
            "#,
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.tenant_id)
        .bind(row.monitor_id)
        .bind(row.steps)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id_for_user(
        &self,
        id: EscalationPolicyId,
        user_id: UserId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        let row = sqlx::query_as::<_, EscalationPolicyRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM escalation_policies \
             WHERE id = $1 AND user_id = $2"
        ))
        .bind(id.0)
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(EscalationPolicyRow::into_domain).transpose()
    }

    async fn find_effective(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        // Try monitor-specific first
        let row = sqlx::query_as::<_, EscalationPolicyRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM escalation_policies \
             WHERE monitor_id = $1 AND user_id = $2 \
             ORDER BY created_at DESC LIMIT 1"
        ))
        .bind(monitor_id.0)
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            return row.into_domain().map(Some);
        }

        // Fall back to global (monitor_id IS NULL)
        let row = sqlx::query_as::<_, EscalationPolicyRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM escalation_policies \
             WHERE monitor_id IS NULL AND user_id = $1 \
             ORDER BY created_at DESC LIMIT 1"
        ))
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(EscalationPolicyRow::into_domain).transpose()
    }

    async fn list_for_user(&self, user_id: UserId) -> RepoResult<Vec<EscalationPolicy>> {
        let rows = sqlx::query_as::<_, EscalationPolicyRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM escalation_policies \
             WHERE user_id = $1 ORDER BY created_at DESC"
        ))
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(EscalationPolicyRow::into_domain)
            .collect()
    }

    async fn find_by_id(&self, id: EscalationPolicyId) -> RepoResult<Option<EscalationPolicy>> {
        let row = sqlx::query_as::<_, EscalationPolicyRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM escalation_policies WHERE id = $1"
        ))
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(EscalationPolicyRow::into_domain).transpose()
    }

    async fn list_for_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<EscalationPolicy>> {
        let rows = sqlx::query_as::<_, EscalationPolicyRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM escalation_policies \
             WHERE tenant_id = $1 ORDER BY created_at DESC"
        ))
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(EscalationPolicyRow::into_domain)
            .collect()
    }
}
