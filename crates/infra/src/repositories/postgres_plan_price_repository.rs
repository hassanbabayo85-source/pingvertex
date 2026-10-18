use std::str::FromStr;

use application::ports::{PlanPriceRepoResult, PlanPriceRepository};
use async_trait::async_trait;
use domain::PlanTier;
use domain::entities::PlanPrice;
use domain::money::{BillingInterval, Currency, Money};
use sqlx::{PgPool, Row};

pub struct PostgresPlanPriceRepository {
    pool: PgPool,
}

impl PostgresPlanPriceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn boxed<E>(err: E) -> Box<dyn std::error::Error + Send + Sync>
where
    E: std::error::Error + Send + Sync + 'static,
{
    Box::new(err)
}

#[async_trait]
impl PlanPriceRepository for PostgresPlanPriceRepository {
    async fn find_effective(
        &self,
        plan_tier: PlanTier,
        currency: Currency,
        billing_interval: BillingInterval,
    ) -> PlanPriceRepoResult<Option<PlanPrice>> {
        let row = sqlx::query(
            r#"
            SELECT pp.id, pp.plan_id, pp.currency, pp.billing_interval,
                   pp.amount_minor, pp.version, pp.effective_from,
                   pp.effective_until, pp.created_at
            FROM plan_prices pp
            JOIN plans p ON p.id = pp.plan_id
            WHERE p.tier = $1
              AND pp.currency = $2
              AND pp.billing_interval = $3
              AND pp.effective_from <= now()
              AND (pp.effective_until IS NULL OR pp.effective_until > now())
            ORDER BY pp.version DESC
            LIMIT 1
            "#,
        )
        .bind(plan_tier.as_str())
        .bind(currency.as_str())
        .bind(billing_interval.as_str())
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let currency_text: String = row.try_get("currency")?;
        let interval_text: String = row.try_get("billing_interval")?;
        let parsed_currency = Currency::from_str(&currency_text).map_err(boxed)?;
        let parsed_interval = BillingInterval::from_str(&interval_text).map_err(boxed)?;
        let amount_minor: i64 = row.try_get("amount_minor")?;

        Ok(Some(PlanPrice {
            id: row.try_get("id")?,
            plan_id: row.try_get("plan_id")?,
            currency: parsed_currency,
            billing_interval: parsed_interval,
            amount: Money::new(amount_minor, parsed_currency).map_err(boxed)?,
            version: row.try_get("version")?,
            effective_from: row.try_get("effective_from")?,
            effective_until: row.try_get("effective_until")?,
            created_at: row.try_get("created_at")?,
        }))
    }
}
