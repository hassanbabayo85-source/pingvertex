use application::ports::{
    TenantRetentionRepoResult as RepoResult, TenantRetentionSettingsRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::TenantRetentionSettings;
use domain::shared::ids::TenantId;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone)]
pub struct PostgresTenantRetentionSettingsRepository {
    pool: PgPool,
}

impl PostgresTenantRetentionSettingsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct Row {
    tenant_id: uuid::Uuid,
    retention_days: i32,
    source: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Row {
    fn into_domain(self) -> Result<TenantRetentionSettings, String> {
        Ok(TenantRetentionSettings {
            tenant_id: TenantId(self.tenant_id),
            retention_days: self.retention_days as u32,
            source: self.source.parse()?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

const SELECT_COLUMNS: &str =
    "tenant_id, retention_days, source, created_at, updated_at";

#[async_trait]
impl TenantRetentionSettingsRepository for PostgresTenantRetentionSettingsRepository {
    async fn get(&self, tenant_id: TenantId) -> RepoResult<Option<TenantRetentionSettings>> {
        let q = format!(
            "SELECT {SELECT_COLUMNS} FROM tenant_retention_settings WHERE tenant_id = $1"
        );
        let row = sqlx::query_as::<_, Row>(&q)
            .bind(tenant_id.0)
            .fetch_optional(&self.pool)
            .await?;
        row.map(Row::into_domain)
            .transpose()
            .map_err(|e| e.into())
    }

    async fn upsert(&self, settings: &TenantRetentionSettings) -> RepoResult<()> {
        // Insert, or update only if the new source has strictly higher
        // priority than the existing one. This preserves an active
        // `Addon` row when a `Default` writer runs.
        sqlx::query(
            "INSERT INTO tenant_retention_settings
                 (tenant_id, retention_days, source, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (tenant_id) DO UPDATE
                 SET retention_days = EXCLUDED.retention_days,
                     source = EXCLUDED.source,
                     updated_at = EXCLUDED.updated_at
                 WHERE CASE EXCLUDED.source
                         WHEN 'addon' THEN 2
                         WHEN 'legacy' THEN 1
                         ELSE 0
                       END
                     > CASE tenant_retention_settings.source
                         WHEN 'addon' THEN 2
                         WHEN 'legacy' THEN 1
                         ELSE 0
                       END",
        )
        .bind(settings.tenant_id.0)
        .bind(settings.retention_days as i32)
        .bind(settings.source.as_str())
        .bind(settings.created_at)
        .bind(settings.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, tenant_id: TenantId) -> RepoResult<()> {
        sqlx::query("DELETE FROM tenant_retention_settings WHERE tenant_id = $1")
            .bind(tenant_id.0)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn effective_retention(
        &self,
        tenant_id: TenantId,
        default_retention_days: u32,
        max_retention_days: u32,
    ) -> RepoResult<u32> {
        let row = self.get(tenant_id).await?;
        let value = match row {
            Some(s) if s.retention_days > 0 => s.retention_days,
            _ => default_retention_days,
        };
        Ok(value.min(max_retention_days).max(1))
    }
}
