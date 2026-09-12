use application::ports::{
    PAYGPricingRepository, PaygPricingRepoResult, PricingContext, PricingRuleContext,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PAYGPricing;
use sqlx::PgPool;

fn account_type_str(a: &domain::AccountType) -> String {
    match a {
        domain::AccountType::Individual => "individual".to_owned(),
        domain::AccountType::Company => "company".to_owned(),
    }
}

fn monitor_kind_str(k: &domain::MonitorKind) -> String {
    match k {
        domain::MonitorKind::Http => "http".to_owned(),
        domain::MonitorKind::Tcp => "tcp".to_owned(),
        domain::MonitorKind::Dns => "dns".to_owned(),
        domain::MonitorKind::Heartbeat => "heartbeat".to_owned(),
        domain::MonitorKind::Flow => "flow".to_owned(),
    }
}

#[derive(Debug, Clone)]
pub struct PostgresPAYGPricingRepository {
    pool: PgPool,
}

impl PostgresPAYGPricingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct PricingRow {
    id: uuid::Uuid,
    version: i64,
    operation: String,
    unit_name: String,
    unit_price: i64,
    currency: String,
    effective_from: DateTime<Utc>,
    effective_until: Option<DateTime<Utc>>,
    active: bool,
    // Context columns
    account_type: Option<String>,
    region: Option<String>,
    monitor_kind: Option<String>,
    min_quantity: Option<i64>,
    max_quantity: Option<i64>,
}

impl PricingRow {
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

    fn specificity(&self) -> i32 {
        let mut s = 0;
        if self.account_type.is_some() {
            s += 1;
        }
        if self.region.is_some() {
            s += 1;
        }
        if self.monitor_kind.is_some() {
            s += 1;
        }
        if self.min_quantity.is_some() || self.max_quantity.is_some() {
            s += 1;
        }
        s
    }
}

#[async_trait]
impl PAYGPricingRepository for PostgresPAYGPricingRepository {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let row = sqlx::query_as::<_, PricingRow>(
            r#"
            SELECT * FROM payg_pricing_configs
            WHERE operation = $1
              AND active = true
              AND effective_from <= now()
              AND (effective_until IS NULL OR effective_until > now())
              AND account_type IS NULL
              AND region IS NULL
              AND monitor_kind IS NULL
              AND min_quantity IS NULL
              AND max_quantity IS NULL
            "#,
        )
        .bind(operation)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_domain()))
    }

    async fn find_active_for_context(
        &self,
        operation: &str,
        context: &PricingContext,
    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let mut rows = sqlx::query_as::<_, PricingRow>(
            r#"
            SELECT * FROM payg_pricing_configs
            WHERE operation = $1
              AND active = true
              AND effective_from <= now()
              AND (effective_until IS NULL OR effective_until > now())
              AND (account_type IS NULL OR account_type = $2)
              AND (region IS NULL OR region = $3)
              AND (monitor_kind IS NULL OR monitor_kind = $4)
              AND (min_quantity IS NULL OR min_quantity <= $5)
              AND (max_quantity IS NULL OR max_quantity >= $5)
            "#,
        )
        .bind(operation)
        .bind(context.account_type.as_ref().map(account_type_str))
        .bind(context.region.as_ref().map(|r| r.as_str().to_string()))
        .bind(context.monitor_kind.as_ref().map(monitor_kind_str))
        .bind(context.quantity as i64)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Ok(None);
        }

        rows.sort_by(|a, b| {
            b.specificity()
                .cmp(&a.specificity())
                .then_with(|| b.effective_from.cmp(&a.effective_from))
                .then_with(|| b.version.cmp(&a.version))
        });
        Ok(rows.into_iter().next().map(PricingRow::into_domain))
    }

    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
        let rows = sqlx::query_as::<_, PricingRow>(
            r#"
            SELECT * FROM payg_pricing_configs
            WHERE active = true
              AND NOT EXISTS (
                  SELECT 1
                  FROM payg_retired_operations retired
                  WHERE retired.operation = payg_pricing_configs.operation
              )
              AND effective_from <= now()
              AND (effective_until IS NULL OR effective_until > now())
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
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
        let mut tx = self.pool.begin().await?;
        if operation.trim().is_empty() || unit_name.trim().is_empty() || currency.trim().is_empty()
        {
            return Err("pricing operation, unit and currency are required".into());
        }
        if unit_price <= 0 {
            return Err("PAYG unit price must be positive".into());
        }
        if context
            .min_quantity
            .zip(context.max_quantity)
            .is_some_and(|(a, b)| a > b)
        {
            return Err("minimum quantity cannot exceed maximum quantity".into());
        }
        let account_type = context.account_type.as_ref().map(account_type_str);
        let region = context.region.as_ref().map(|r| r.as_str().to_string());
        let monitor_kind = context.monitor_kind.as_ref().map(monitor_kind_str);
        let min_q = context.min_quantity.map(|v| v as i64);
        let max_q = context.max_quantity.map(|v| v as i64);
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(operation)
            .execute(&mut *tx)
            .await?;
        let version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version),0)+1 FROM payg_pricing_configs WHERE operation=$1",
        )
        .bind(operation)
        .fetch_one(&mut *tx)
        .await?;
        let ambiguous: Option<i64> = sqlx::query_scalar(
            r#"SELECT 1 FROM payg_pricing_configs
            WHERE operation=$1 AND active=true AND effective_from=$2
              AND account_type IS NOT DISTINCT FROM $3
              AND region IS NOT DISTINCT FROM $4
              AND monitor_kind IS NOT DISTINCT FROM $5
              AND min_quantity IS NOT DISTINCT FROM $6
              AND max_quantity IS NOT DISTINCT FROM $7 LIMIT 1"#,
        )
        .bind(operation)
        .bind(effective_from)
        .bind(&account_type)
        .bind(&region)
        .bind(&monitor_kind)
        .bind(min_q)
        .bind(max_q)
        .fetch_optional(&mut *tx)
        .await?;
        if ambiguous.is_some() {
            return Err(
                "ambiguous PAYG pricing rule already exists for this context and effective time"
                    .into(),
            );
        }
        if effective_from <= Utc::now() {
            let existing = sqlx::query_as::<_, PricingRow>(
                "SELECT * FROM payg_pricing_configs WHERE operation=$1 AND active=true",
            )
            .bind(operation)
            .fetch_all(&mut *tx)
            .await?;
            let same_context = |p: &PricingRow| {
                p.account_type.as_deref() == account_type.as_deref()
                    && p.region.as_deref() == region.as_deref()
                    && p.monitor_kind.as_deref() == monitor_kind.as_deref()
                    && p.min_quantity == min_q
                    && p.max_quantity == max_q
            };
            let overlap = |p: &PricingRow| {
                let account = p.account_type.is_none()
                    || account_type.is_none()
                    || p.account_type == account_type;
                let region_match = p.region.is_none() || region.is_none() || p.region == region;
                let kind = p.monitor_kind.is_none()
                    || monitor_kind.is_none()
                    || p.monitor_kind == monitor_kind;
                let p_min = p.min_quantity.unwrap_or(0);
                let p_max = p.max_quantity.unwrap_or(i64::MAX);
                let n_min = min_q.unwrap_or(0);
                let n_max = max_q.unwrap_or(i64::MAX);
                account && region_match && kind && p_min <= n_max && n_min <= p_max
            };
            let new_specificity = [
                account_type.is_some(),
                region.is_some(),
                monitor_kind.is_some(),
                min_q.is_some() || max_q.is_some(),
            ]
            .into_iter()
            .filter(|x| *x)
            .count() as i32;
            if existing.iter().any(|p| {
                p.effective_from <= Utc::now()
                    && p.specificity() == new_specificity
                    && !same_context(p)
                    && overlap(p)
            }) {
                return Err("ambiguous overlapping PAYG pricing rule at equal specificity".into());
            }
        }
        let row=sqlx::query_as::<_,PricingRow>(r#"INSERT INTO payg_pricing_configs
            (id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active,account_type,region,monitor_kind,min_quantity,max_quantity)
            VALUES(gen_random_uuid(),$1,$2,$3,$4,$5,$6,NULL,true,$7,$8,$9,$10,$11) RETURNING *"#)
            .bind(version).bind(operation).bind(unit_name).bind(unit_price).bind(currency).bind(effective_from)
            .bind(&account_type).bind(&region).bind(&monitor_kind).bind(min_q).bind(max_q)
            .fetch_one(&mut *tx).await?;
        tx.commit().await?;

        Ok(row.into_domain())
    }
}
