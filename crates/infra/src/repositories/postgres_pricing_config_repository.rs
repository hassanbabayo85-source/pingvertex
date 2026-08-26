use application::ports::PricingConfigRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::pricing::{FreeTierDefaults, PricingConfig};
use rust_decimal::Decimal;
use sqlx::{FromRow, PgPool};
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
    min_paid_monthly_price: Decimal,
    free_monitors: i64,
    free_interval_seconds: i32,
    free_retention_days: i32,
    created_at: DateTime<Utc>,
    effective_at: DateTime<Utc>,
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
            min_paid_monthly_price: self.min_paid_monthly_price,
            free_tier: FreeTierDefaults {
                monitors: self.free_monitors as u64,
                interval_seconds: self.free_interval_seconds as u32,
                retention_days: self.free_retention_days as u32,
            },
            currency: self.currency,
            effective_at: self.effective_at,
        }
    }
}

#[async_trait]
impl PricingConfigRepository for PostgresPricingConfigRepository {
    async fn find_active_config(&self) -> RepoResult<Option<PricingConfig>> {
        let row = sqlx::query_as::<_, PricingConfigRow>(
            "SELECT id, version, currency, cost_per_check, effective_storage_bytes_per_check, \
             cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold, backup_multiplier, \
             markup_multiplier, min_interval_seconds, max_monitors, max_retention_days, \
             min_paid_monthly_price, free_monitors, free_interval_seconds, free_retention_days, \
             created_at, effective_at \
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
             markup_multiplier, min_interval_seconds, max_monitors, max_retention_days, \
             min_paid_monthly_price, free_monitors, free_interval_seconds, free_retention_days, \
             created_at, effective_at \
             FROM pricing_configs \
             WHERE version = $1",
        )
        .bind(version as i32)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(PricingConfigRow::into_domain))
    }

    async fn create_version(&self, config: &PricingConfig) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO pricing_configs (id, version, currency, cost_per_check, \
             effective_storage_bytes_per_check, cost_per_gb_hot, cost_per_gb_warm, \
             cost_per_gb_cold, backup_multiplier, markup_multiplier, min_interval_seconds, \
             max_monitors, max_retention_days, min_paid_monthly_price, free_monitors, \
             free_interval_seconds, free_retention_days, effective_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)",
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
        .bind(config.min_paid_monthly_price)
        .bind(config.free_tier.monitors as i64)
        .bind(config.free_tier.interval_seconds as i32)
        .bind(config.free_tier.retention_days as i32)
        .bind(config.effective_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_versions(&self) -> RepoResult<Vec<PricingConfig>> {
        let rows = sqlx::query_as::<_, PricingConfigRow>(
            "SELECT id, version, currency, cost_per_check, effective_storage_bytes_per_check, \
             cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold, backup_multiplier, \
             markup_multiplier, min_interval_seconds, max_monitors, max_retention_days, \
             min_paid_monthly_price, free_monitors, free_interval_seconds, free_retention_days, \
             created_at, effective_at \
             FROM pricing_configs ORDER BY version",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(PricingConfigRow::into_domain)
            .collect())
    }
}
