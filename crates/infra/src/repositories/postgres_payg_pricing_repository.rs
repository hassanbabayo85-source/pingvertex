use application::ports::{
    PAYGPricingRepository, PaygPricingRepoResult, PricingContext, PricingRuleContext,
};
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
               AND account_type IS NULL AND region IS NULL AND monitor_kind IS NULL
               AND min_quantity IS NULL AND max_quantity IS NULL
             ORDER BY version DESC LIMIT 1",
        ).bind(operation).fetch_optional(&self.pool).await?;
        Ok(row.map(Row::into_domain))
    }
    async fn find_active_for_context(
        &self,
        operation: &str,
        context: &PricingContext,
    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let account_type = context.account_type.map(|v| v.as_str());
        let region = context.region.as_ref().map(|v| v.as_str());
        let monitor_kind = context
            .monitor_kind
            .map(|v| format!("{:?}", v).to_ascii_lowercase());
        let quantity = i64::try_from(context.quantity)
            .map_err(|_| "pricing quantity exceeds supported range")?;
        let row = sqlx::query_as::<_, Row>(
            "SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active
             FROM payg_pricing_configs
             WHERE operation=$1 AND active=TRUE AND effective_from<=now()
               AND (effective_until IS NULL OR effective_until>now())
               AND (account_type IS NULL OR account_type=$2)
               AND (region IS NULL OR region=$3)
               AND (monitor_kind IS NULL OR monitor_kind=$4)
               AND (min_quantity IS NULL OR min_quantity <= $5)
               AND (max_quantity IS NULL OR max_quantity >= $5)
             ORDER BY
               ((account_type IS NOT NULL)::int + (region IS NOT NULL)::int + (monitor_kind IS NOT NULL)::int
                + (min_quantity IS NOT NULL)::int + (max_quantity IS NOT NULL)::int) DESC,
               version DESC
             LIMIT 1",
        )
        .bind(operation)
        .bind(account_type)
        .bind(region)
        .bind(monitor_kind.as_deref())
        .bind(quantity)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Row::into_domain))
    }

    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
        let rows=sqlx::query_as::<_,Row>("SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active FROM payg_pricing_configs WHERE active=TRUE AND effective_from<=now() AND (effective_until IS NULL OR effective_until>now())
               AND account_type IS NULL AND region IS NULL AND monitor_kind IS NULL AND min_quantity IS NULL AND max_quantity IS NULL
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
        self.create_version_with_context(
            operation,
            unit_name,
            unit_price,
            currency,
            effective_from,
            &PricingRuleContext::default(),
        )
        .await
    }

    async fn create_version_with_context(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
        context: &PricingRuleContext,
    ) -> PaygPricingRepoResult<PAYGPricing> {
        if !domain::PaygOperation::is_canonical(operation) {
            return Err("unsupported PAYG operation".into());
        }
        if unit_price <= 0 || currency != "USD" {
            return Err("invalid PAYG price".into());
        }
        if context
            .min_quantity
            .zip(context.max_quantity)
            .is_some_and(|(min, max)| min > max)
            || context.min_quantity.is_some_and(|v| v > i64::MAX as u64)
            || context.max_quantity.is_some_and(|v| v > i64::MAX as u64)
        {
            return Err("invalid pricing quantity range".into());
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('payg-pricing-version')::bigint)")
            .execute(&mut *tx)
            .await?;
        let version: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) + 1 FROM payg_pricing_configs")
                .fetch_one(&mut *tx)
                .await?;
        let id = Uuid::new_v4();
        let account_type = context.account_type.map(|v| v.as_str());
        let region = context.region.as_ref().map(|v| v.as_str());
        let monitor_kind = context
            .monitor_kind
            .map(|v| format!("{:?}", v).to_ascii_lowercase());
        let row = sqlx::query_as::<_, Row>(
            r#"INSERT INTO payg_pricing_configs
               (id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active,account_type,region,monitor_kind,min_quantity,max_quantity)
               VALUES($1,$2,$3,$4,$5,$6,$7,NULL,TRUE,$8,$9,$10,$11,$12)
               RETURNING id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active"#
        )
        .bind(id).bind(version).bind(operation).bind(unit_name).bind(unit_price).bind(currency)
        .bind(effective_from).bind(account_type).bind(region).bind(monitor_kind)
        .bind(context.min_quantity.map(|v| v as i64))
        .bind(context.max_quantity.map(|v| v as i64))
        .fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }
}
