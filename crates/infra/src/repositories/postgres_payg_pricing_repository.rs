use application::ports::{PAYGPricingRepository, PaygPricingRepoResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PAYGPricing;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresPAYGPricingRepository {
    pool: PgPool,
}
impl PostgresPAYGPricingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[derive(FromRow)]
struct Row {
    id: Uuid,
    version: i64,
    operation: String,
    unit_name: String,
    unit_price: i64,
    currency: String,
    effective_from: DateTime<Utc>,
    effective_until: Option<DateTime<Utc>>,
    active: bool,
}
impl Row {
    fn into_domain(self) -> PAYGPricing {
        PAYGPricing {
            id: self.id,
            version: self.version,
            operation: self.operation,
            unit_name: self.unit_name,
            unit_price: self.unit_price,
            currency: self.currency,
            effective_from: self.effective_from,
            effective_until: self.effective_until,
            active: self.active,
        }
    }
}
#[async_trait]
impl PAYGPricingRepository for PostgresPAYGPricingRepository {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let row = sqlx::query_as::<_, Row>(
            "SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active
             FROM payg_pricing_configs
             WHERE operation=$1 AND active=TRUE AND effective_from<=now()
               AND (effective_until IS NULL OR effective_until>now())
             ORDER BY version DESC LIMIT 1",
        ).bind(operation).fetch_optional(&self.pool).await?;
        Ok(row.map(Row::into_domain))
    }
    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
        let rows=sqlx::query_as::<_,Row>("SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active FROM payg_pricing_configs WHERE active=TRUE AND effective_from<=now() AND (effective_until IS NULL OR effective_until>now()) ORDER BY operation,version DESC").fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Row::into_domain).collect())
    }
}
