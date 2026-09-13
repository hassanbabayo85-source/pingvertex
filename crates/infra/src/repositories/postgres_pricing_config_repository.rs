use application::ports::PricingConfigRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::pricing::{FreeTierDefaults, PricingConfig};
use domain::{
    PricingAuditLogEntry,
    shared::ids::{AuditLogEntryId, UserId},
};
use rust_decimal::Decimal;
use sqlx::{FromRow, PgPool, Postgres};
use std::error::Error;
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresPricingConfigRepository {
    pool: PgPool,
}

impl PostgresPricingConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts an audit log entry using the given executor (pool or transaction).
    async fn insert_audit_log<'e, E>(executor: E, entry: &PricingAuditLogEntry) -> RepoResult<()>
    where
        E: sqlx::Executor<'e, Database = Postgres>,
    {
        sqlx::query(
            "INSERT INTO pricing_audit_logs \
             (id, version, field_name, old_value, new_value, changed_by, changed_at, reason) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(entry.id.0)
        .bind(entry.version as i32)
        .bind(&entry.field_name)
        .bind(&entry.old_value)
        .bind(&entry.new_value)
        .bind(entry.changed_by.map(|u| u.0))
        .bind(entry.changed_at)
        .bind(&entry.reason)
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
struct PricingConfigRow {
    id: Uuid,
    version: i32,
    currency: String,
    cost_per_check: Decimal,
    effective_storage_bytes_per_check: i32,
    cost_per_gb_hot: Decimal,
    cost_per_gb_warm: Decimal,
    cost_per_gb_cold: Decimal,
    backup_multiplier: Decimal,
    markup_multiplier: Decimal,
    min_interval_seconds: i32,
    max_monitors: i64,
    max_retention_days: i32,
    default_retention_days: i32,
    min_paid_monthly_price: Decimal,
    free_monitors: i64,
    free_interval_seconds: i32,
    free_retention_days: i32,
    avg_billable_network_bytes_per_check: i64,
    cost_per_gb_egress: Decimal,
    cost_safety_buffer: Decimal,
    sms_markup_multiplier: Decimal,
    enterprise_sla_monthly_price: Decimal,
    created_at: DateTime<Utc>,
    effective_at: DateTime<Utc>,
    created_by: Option<Uuid>,
}

impl PricingConfigRow {
    fn into_domain(self) -> PricingConfig {
        PricingConfig {
            version: self.version as u32,
            cost_per_check: self.cost_per_check,
            effective_storage_bytes_per_check: self.effective_storage_bytes_per_check as u32,
            cost_per_gb_hot: self.cost_per_gb_hot,
            cost_per_gb_warm: self.cost_per_gb_warm,
            cost_per_gb_cold: self.cost_per_gb_cold,
            backup_multiplier: self.backup_multiplier,
            markup_multiplier: self.markup_multiplier,
            min_interval_seconds: self.min_interval_seconds as u32,
            max_monitors: self.max_monitors as u64,
            max_retention_days: self.max_retention_days as u32,
            default_retention_days: self.default_retention_days as u32,
            min_paid_monthly_price: self.min_paid_monthly_price,
            free_tier: FreeTierDefaults {
                monitors: self.free_monitors as u64,
                interval_seconds: self.free_interval_seconds as u32,
                retention_days: self.free_retention_days as u32,
            },
            currency: self.currency,
            effective_at: self.effective_at,
            avg_billable_network_bytes_per_check: self.avg_billable_network_bytes_per_check as u64,
            cost_per_gb_egress: self.cost_per_gb_egress,
            cost_safety_buffer: self.cost_safety_buffer,
            sms_markup_multiplier: self.sms_markup_multiplier,
            enterprise_sla_monthly_price: self.enterprise_sla_monthly_price,
            created_by: self.created_by.map(UserId),
        }
    }
}

#[derive(Debug, FromRow)]
struct PricingAuditLogRow {
    id: Uuid,
    version: i32,
    field_name: String,
    old_value: Option<String>,
    new_value: Option<String>,
    changed_by: Option<Uuid>,
    changed_at: DateTime<Utc>,
    reason: Option<String>,
}

impl PricingAuditLogRow {
    fn into_domain(self) -> PricingAuditLogEntry {
        PricingAuditLogEntry {
            id: AuditLogEntryId(self.id),
            version: self.version as u32,
            field_name: self.field_name,
            old_value: self.old_value,
            new_value: self.new_value,
            changed_by: self.changed_by.map(UserId),
            changed_at: self.changed_at,
            reason: self.reason,
        }
    }
}

#[async_trait]
impl PricingConfigRepository for PostgresPricingConfigRepository {
    async fn find_active_config(&self) -> RepoResult<Option<PricingConfig>> {
        let row = sqlx::query_as::<_, PricingConfigRow>(
            "SELECT id, version, currency, cost_per_check, effective_storage_bytes_per_check, \
             cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold, backup_multiplier, \
             markup_multiplier, min_interval_seconds, max_monitors, max_retention_days, default_retention_days, \
             min_paid_monthly_price, free_monitors, free_interval_seconds, free_retention_days, \
             avg_billable_network_bytes_per_check, cost_per_gb_egress, cost_safety_buffer, sms_markup_multiplier, enterprise_sla_monthly_price, \
             created_at, effective_at, created_by \
             FROM pricing_configs \
             WHERE effective_at <= now() \
             ORDER BY version DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(PricingConfigRow::into_domain))
    }

    async fn find_by_version(&self, version: u32) -> RepoResult<Option<PricingConfig>> {
        let row = sqlx::query_as::<_, PricingConfigRow>(
            "SELECT id, version, currency, cost_per_check, effective_storage_bytes_per_check, \
             cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold, backup_multiplier, \
             markup_multiplier, min_interval_seconds, max_monitors, max_retention_days, default_retention_days, \
             min_paid_monthly_price, free_monitors, free_interval_seconds, free_retention_days, \
             avg_billable_network_bytes_per_check, cost_per_gb_egress, cost_safety_buffer, sms_markup_multiplier, enterprise_sla_monthly_price, \
             created_at, effective_at, created_by \
             FROM pricing_configs \
             WHERE version = $1",
        )
        .bind(version as i32)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(PricingConfigRow::into_domain))
    }

    async fn create_version(
        &self,
        config: &PricingConfig,
        reason: Option<String>,
    ) -> RepoResult<()> {
        let mut tx = self.pool.begin().await?;

        // Get previous version for audit log old_value
        let prev_version: Option<i32> =
            sqlx::query_scalar("SELECT MAX(version) FROM pricing_configs")
                .fetch_one(&mut *tx)
                .await?;

        sqlx::query(
            "INSERT INTO pricing_configs (id, version, currency, cost_per_check, \
             effective_storage_bytes_per_check, cost_per_gb_hot, cost_per_gb_warm, \
             cost_per_gb_cold, backup_multiplier, markup_multiplier, min_interval_seconds, \
             max_monitors, max_retention_days, default_retention_days, min_paid_monthly_price, free_monitors, \
             free_interval_seconds, free_retention_days, avg_billable_network_bytes_per_check, \
             cost_per_gb_egress, cost_safety_buffer, sms_markup_multiplier, enterprise_sla_monthly_price, effective_at, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25)",
        )
        .bind(Uuid::new_v4())
        .bind(config.version as i32)
        .bind(&config.currency)
        .bind(config.cost_per_check)
        .bind(config.effective_storage_bytes_per_check as i32)
        .bind(config.cost_per_gb_hot)
        .bind(config.cost_per_gb_warm)
        .bind(config.cost_per_gb_cold)
        .bind(config.backup_multiplier)
        .bind(config.markup_multiplier)
        .bind(config.min_interval_seconds as i32)
        .bind(config.max_monitors as i64)
        .bind(config.max_retention_days as i32)
        .bind(config.default_retention_days as i32)
        .bind(config.min_paid_monthly_price)
        .bind(config.free_tier.monitors as i64)
        .bind(config.free_tier.interval_seconds as i32)
        .bind(config.free_tier.retention_days as i32)
        .bind(config.avg_billable_network_bytes_per_check as i64)
        .bind(config.cost_per_gb_egress)
        .bind(config.cost_safety_buffer)
        .bind(config.sms_markup_multiplier)
        .bind(config.enterprise_sla_monthly_price)
        .bind(config.effective_at)
        .bind(config.created_by.map(|u| u.0))
        .execute(&mut *tx)
        .await?;

        // Create audit log entry
        let entry = PricingAuditLogEntry::new(
            AuditLogEntryId::new(),
            config.version,
            "version".to_string(),
            prev_version.map(|v| v.to_string()),
            Some(config.version.to_string()),
            config.created_by,
            reason,
        );
        Self::insert_audit_log(&mut *tx, &entry).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn record_audit_log(&self, entry: &PricingAuditLogEntry) -> RepoResult<()> {
        Self::insert_audit_log(&self.pool, entry).await
    }

    async fn list_versions(&self) -> RepoResult<Vec<PricingConfig>> {
        let rows = sqlx::query_as::<_, PricingConfigRow>(
            "SELECT id, version, currency, cost_per_check, effective_storage_bytes_per_check, \
             cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold, backup_multiplier, \
             markup_multiplier, min_interval_seconds, max_monitors, max_retention_days, default_retention_days, \
             min_paid_monthly_price, free_monitors, free_interval_seconds, free_retention_days, \
             avg_billable_network_bytes_per_check, cost_per_gb_egress, cost_safety_buffer, sms_markup_multiplier, enterprise_sla_monthly_price, \
             created_at, effective_at, created_by \
             FROM pricing_configs ORDER BY version",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(PricingConfigRow::into_domain)
            .collect())
    }

    async fn list_audit_logs(&self) -> RepoResult<Vec<PricingAuditLogEntry>> {
        let rows = sqlx::query_as::<_, PricingAuditLogRow>(
            "SELECT id, version, field_name, old_value, new_value, changed_by, changed_at, reason \
             FROM pricing_audit_logs \
             ORDER BY changed_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(PricingAuditLogRow::into_domain)
            .collect())
    }
}
