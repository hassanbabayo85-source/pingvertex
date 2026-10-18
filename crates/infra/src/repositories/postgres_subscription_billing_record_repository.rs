use std::str::FromStr;

use application::ports::subscription_billing_record_repository::{
    BillingRecordStatus, InsertBillingRecordOutcome, RepoResult, SubscriptionBillingRecord,
    SubscriptionBillingRecordRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PlanTier;
use domain::money::{BillingInterval, Currency};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

pub struct PostgresSubscriptionBillingRecordRepository {
    pool: PgPool,
}

impl PostgresSubscriptionBillingRecordRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct Row {
    id: Uuid,
    subscription_id: Uuid,
    tenant_id: Uuid,
    plan_tier: String,
    billing_interval: String,
    currency: String,
    amount_minor: i64,
    status: String,
    provider: String,
    provider_reference: String,
    provider_payment_id: Option<String>,
    provider_subscription_code: Option<String>,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn boxed<E>(err: E) -> Box<dyn std::error::Error + Send + Sync>
where
    E: std::error::Error + Send + Sync + 'static,
{
    Box::new(err)
}

fn into_domain(row: Row) -> RepoResult<SubscriptionBillingRecord> {
    let plan_tier = row.plan_tier.parse::<PlanTier>().map_err(boxed)?;
    let billing_interval = BillingInterval::from_str(&row.billing_interval).map_err(boxed)?;
    let currency = Currency::from_str(&row.currency).map_err(boxed)?;
    let status = BillingRecordStatus::from_str(&row.status)?;
    Ok(SubscriptionBillingRecord {
        id: row.id,
        subscription_id: row.subscription_id,
        tenant_id: row.tenant_id,
        plan_tier,
        billing_interval,
        currency,
        amount_minor: row.amount_minor,
        status,
        provider: row.provider,
        provider_reference: row.provider_reference,
        provider_payment_id: row.provider_payment_id,
        provider_subscription_code: row.provider_subscription_code,
        period_start: row.period_start,
        period_end: row.period_end,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[async_trait]
impl SubscriptionBillingRecordRepository for PostgresSubscriptionBillingRecordRepository {
    async fn try_insert(
        &self,
        record: &SubscriptionBillingRecord,
    ) -> RepoResult<InsertBillingRecordOutcome> {
        let inserted = sqlx::query(
            r#"
            INSERT INTO subscription_billing_records
                (id, subscription_id, tenant_id, plan_tier, billing_interval,
                 currency, amount_minor, status, provider, provider_reference,
                 provider_payment_id, provider_subscription_code,
                 period_start, period_end, created_at, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
            ON CONFLICT (provider, provider_reference) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(record.id)
        .bind(record.subscription_id)
        .bind(record.tenant_id)
        .bind(record.plan_tier.as_str())
        .bind(record.billing_interval.as_str())
        .bind(record.currency.as_str())
        .bind(record.amount_minor)
        .bind(record.status.as_str())
        .bind(&record.provider)
        .bind(&record.provider_reference)
        .bind(&record.provider_payment_id)
        .bind(&record.provider_subscription_code)
        .bind(record.period_start)
        .bind(record.period_end)
        .bind(record.created_at)
        .bind(record.updated_at)
        .fetch_optional(&self.pool)
        .await?;
        Ok(if inserted.is_some() {
            InsertBillingRecordOutcome::Inserted
        } else {
            InsertBillingRecordOutcome::AlreadyExists
        })
    }

    async fn mark_succeeded(
        &self,
        provider: &str,
        provider_reference: &str,
        provider_payment_id: &str,
    ) -> RepoResult<bool> {
        let result = sqlx::query(
            "UPDATE subscription_billing_records \
             SET status = 'succeeded', provider_payment_id = $3, updated_at = now() \
             WHERE provider = $1 AND provider_reference = $2",
        )
        .bind(provider)
        .bind(provider_reference)
        .bind(provider_payment_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_by_provider_reference(
        &self,
        provider: &str,
        provider_reference: &str,
    ) -> RepoResult<Option<SubscriptionBillingRecord>> {
        let row = sqlx::query_as::<_, Row>(
            "SELECT id, subscription_id, tenant_id, plan_tier, billing_interval, \
             currency, amount_minor, status, provider, provider_reference, \
             provider_payment_id, provider_subscription_code, period_start, \
             period_end, created_at, updated_at \
             FROM subscription_billing_records \
             WHERE provider = $1 AND provider_reference = $2",
        )
        .bind(provider)
        .bind(provider_reference)
        .fetch_optional(&self.pool)
        .await?;
        row.map(into_domain).transpose()
    }
}
