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
        let rows=sqlx::query_as::<_,Row>("SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active FROM payg_pricing_configs WHERE active=TRUE AND effective_from<=now() AND (effective_until IS NULL OR effective_until>now())
               AND operation IN ('http_check.one_time','tcp_check.one_time','dns_check.one_time','api_flow_check.one_time','proxy_check.one_time','http_check.monitoring','tcp_check.monitoring','dns_check.monitoring','heartbeat_processing','api_flow_check.monitoring','proxy_check.monitoring','rum_event','sms','storage','network_egress')
               AND version=(SELECT MAX(p2.version) FROM payg_pricing_configs p2 WHERE p2.operation=payg_pricing_configs.operation AND p2.active=TRUE AND p2.effective_from<=now() AND (p2.effective_until IS NULL OR p2.effective_until>now())) ORDER BY operation").fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Row::into_domain).collect())
    }
    async fn create_version(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
    ) -> PaygPricingRepoResult<PAYGPricing> {
        if !domain::PaygOperation::is_canonical(operation) {
            return Err("unsupported PAYG operation".into());
        }
        if unit_price <= 0 || currency != "USD" {
            return Err("invalid PAYG price".into());
        }
        let mut tx = self.pool.begin().await?;
        // Serialize version allocation without mutating historical rows.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('payg-pricing-version')::bigint)")
            .execute(&mut *tx)
            .await?;

        let version: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) + 1 FROM payg_pricing_configs")
                .fetch_one(&mut *tx)
                .await?;

        let id = Uuid::new_v4();
        let row = sqlx::query_as::<_, Row>(
            r#"INSERT INTO payg_pricing_configs
               (id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active)
               VALUES($1,$2,$3,$4,$5,$6,$7,NULL,TRUE)
               RETURNING id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active"#,
        )
        .bind(id)
        .bind(version)
        .bind(operation)
        .bind(unit_name)
        .bind(unit_price)
        .bind(currency)
        .bind(effective_from)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(row.into_domain())
    }
}
