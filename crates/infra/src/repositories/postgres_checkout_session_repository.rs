use std::error::Error;
use std::str::FromStr;

use application::ports::CheckoutSessionRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::CheckoutSession;
use domain::enums::{CheckoutStatus, PlanTier};
use domain::money::BillingInterval;
use domain::shared::ids::{CheckoutSessionId, TenantId};
use rust_decimal::Decimal;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type CheckoutRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresCheckoutSessionRepository {
    pool: PgPool,
}

impl PostgresCheckoutSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct CheckoutSessionRow {
    id: Uuid,
    tenant_id: Uuid,
    plan_tier: Option<String>,
    status: String,
    provider: String,
    provider_checkout_id: Option<String>,
    plan_change_id: Option<Uuid>,
    addon_subscription_id: Option<Uuid>,
    amount: Option<Decimal>,
    currency: Option<String>,
    billing_interval: Option<String>,
    amount_minor: Option<i64>,
    provider_payment_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl CheckoutSessionRow {
    fn from_domain(session: &CheckoutSession) -> Self {
        Self {
            id: session.id.0,
            tenant_id: session.tenant_id.0,
            plan_tier: session.plan_tier.as_ref().map(|t| t.as_str().to_owned()),
            status: session.status.as_str().to_owned(),
            provider: session.provider.clone(),
            provider_checkout_id: session.provider_checkout_id.clone(),
            plan_change_id: session.plan_change_id,
            addon_subscription_id: session.addon_subscription_id,
            amount: session.amount,
            currency: session.currency.clone(),
            billing_interval: session.billing_interval.map(|i| i.as_str().to_owned()),
            amount_minor: session.amount_minor,
            provider_payment_id: session.provider_payment_id.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
        }
    }

    fn into_domain(self) -> CheckoutRepoResult<CheckoutSession> {
        let plan_tier: Option<PlanTier> = match self.plan_tier.as_deref() {
            Some(s) => Some(s.parse().map_err(|err: domain::DomainError| {
                Box::new(err) as Box<dyn Error + Send + Sync>
            })?),
            None => None,
        };
        let status: CheckoutStatus = self
            .status
            .parse()
            .map_err(|err: domain::DomainError| Box::new(err) as Box<dyn Error + Send + Sync>)?;
        let billing_interval: Option<BillingInterval> = match self.billing_interval.as_deref() {
            Some(s) => Some(
                BillingInterval::from_str(s)
                    .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?,
            ),
            None => None,
        };

        Ok(CheckoutSession {
            id: CheckoutSessionId(self.id),
            tenant_id: TenantId(self.tenant_id),
            plan_tier,
            status,
            provider: self.provider,
            provider_checkout_id: self.provider_checkout_id,
            plan_change_id: self.plan_change_id,
            addon_subscription_id: self.addon_subscription_id,
            amount: self.amount,
            currency: self.currency,
            billing_interval,
            amount_minor: self.amount_minor,
            provider_payment_id: self.provider_payment_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[async_trait]
impl CheckoutSessionRepository for PostgresCheckoutSessionRepository {
    async fn create(&self, session: &CheckoutSession) -> CheckoutRepoResult<()> {
        let row = CheckoutSessionRow::from_domain(session);
        sqlx::query(
            "INSERT INTO checkout_sessions \
             (id, tenant_id, plan_tier, status, provider, provider_checkout_id, plan_change_id, addon_subscription_id, amount, currency, billing_interval, amount_minor, provider_payment_id, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
        .bind(row.id)
        .bind(row.tenant_id)
        .bind(row.plan_tier)
        .bind(row.status)
        .bind(row.provider)
        .bind(row.provider_checkout_id)
        .bind(row.plan_change_id)
        .bind(row.addon_subscription_id)
        .bind(row.amount)
        .bind(row.currency)
        .bind(row.billing_interval)
        .bind(row.amount_minor)
        .bind(row.provider_payment_id)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: CheckoutSessionId,
    ) -> CheckoutRepoResult<Option<CheckoutSession>> {
        let row = sqlx::query_as::<_, CheckoutSessionRow>(
            "SELECT id, tenant_id, plan_tier, status, provider, provider_checkout_id, plan_change_id, addon_subscription_id, amount, currency, billing_interval, amount_minor, provider_payment_id, created_at, updated_at \
             FROM checkout_sessions WHERE id = $1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(CheckoutSessionRow::into_domain).transpose()
    }

    async fn update(&self, session: &CheckoutSession) -> CheckoutRepoResult<()> {
        let row = CheckoutSessionRow::from_domain(session);
        sqlx::query(
            "UPDATE checkout_sessions SET \
             plan_tier = $2, status = $3, provider = $4, provider_checkout_id = $5, plan_change_id = $6, addon_subscription_id = $7, amount = $8, currency = $9, billing_interval = $10, amount_minor = $11, provider_payment_id = $12, updated_at = $13 \
             WHERE id = $1",
        )
        .bind(row.id)
        .bind(row.plan_tier)
        .bind(row.status)
        .bind(row.provider)
        .bind(row.provider_checkout_id)
        .bind(row.plan_change_id)
        .bind(row.addon_subscription_id)
        .bind(row.amount)
        .bind(row.currency)
        .bind(row.billing_interval)
        .bind(row.amount_minor)
        .bind(row.provider_payment_id)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_provider_checkout_id(
        &self,
        provider: &str,
        provider_checkout_id: &str,
    ) -> CheckoutRepoResult<Option<CheckoutSession>> {
        let row = sqlx::query_as::<_, CheckoutSessionRow>(
            "SELECT id, tenant_id, plan_tier, status, provider, provider_checkout_id, plan_change_id, addon_subscription_id, amount, currency, billing_interval, amount_minor, provider_payment_id, created_at, updated_at \
             FROM checkout_sessions WHERE provider = $1 AND provider_checkout_id = $2 LIMIT 1",
        )
        .bind(provider)
        .bind(provider_checkout_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(CheckoutSessionRow::into_domain).transpose()
    }
}
