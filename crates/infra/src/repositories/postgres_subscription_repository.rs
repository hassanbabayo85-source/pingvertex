use std::error::Error;

use application::ports::SubscriptionRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    Subscription,
    enums::{PlanTier, SubscriptionStatus},
    shared::ids::{SubscriptionId, TenantId},
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresSubscriptionRepository {
    pool: PgPool,
}

impl PostgresSubscriptionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct SubscriptionRow {
    id: Uuid,
    tenant_id: Uuid,
    plan_tier: String,
    status: String,
    trial_ends_at: Option<DateTime<Utc>>,
    grace_period_ends_at: Option<DateTime<Utc>>,
    current_period_ends_at: Option<DateTime<Utc>>,
    canceled_at: Option<DateTime<Utc>>,
    cancel_at_period_end: bool,
    // Proration Service: usage snapshot columns added by migration
    // 0053, previously unused by any repository.
    monitors: Option<i64>,
    interval_seconds: Option<i32>,
    retention_days: Option<i32>,
    pricing_version: Option<i32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SubscriptionRow {
    fn from_domain(subscription: &Subscription) -> Self {
        Self {
            id: subscription.id.0,
            tenant_id: subscription.tenant_id.0,
            plan_tier: subscription.plan_tier.as_str().to_owned(),
            status: subscription.status.as_str().to_owned(),
            trial_ends_at: subscription.trial_ends_at,
            grace_period_ends_at: subscription.grace_period_ends_at,
            current_period_ends_at: subscription.current_period_ends_at,
            canceled_at: subscription.canceled_at,
            cancel_at_period_end: subscription.cancel_at_period_end,
            monitors: subscription.monitors.map(|m| m as i64),
            interval_seconds: subscription.interval_seconds.map(|i| i as i32),
            retention_days: subscription.retention_days.map(|r| r as i32),
            pricing_version: subscription.pricing_version.map(|v| v as i32),
            created_at: subscription.created_at,
            updated_at: subscription.updated_at,
        }
    }

    fn into_domain(self) -> Result<Subscription, Box<dyn Error + Send + Sync>> {
        let plan_tier: PlanTier = self
            .plan_tier
            .parse()
            .map_err(|err: domain::DomainError| Box::new(err) as Box<dyn Error + Send + Sync>)?;
        let status: SubscriptionStatus = self
            .status
            .parse()
            .map_err(|err: domain::DomainError| Box::new(err) as Box<dyn Error + Send + Sync>)?;

        Ok(Subscription {
            id: SubscriptionId(self.id),
            tenant_id: TenantId(self.tenant_id),
            plan_tier,
            status,
            trial_ends_at: self.trial_ends_at,
            grace_period_ends_at: self.grace_period_ends_at,
            current_period_ends_at: self.current_period_ends_at,
            canceled_at: self.canceled_at,
            cancel_at_period_end: self.cancel_at_period_end,
            monitors: self.monitors.map(|m| m as u64),
            interval_seconds: self.interval_seconds.map(|i| i as u32),
            retention_days: self.retention_days.map(|r| r as u32),
            pricing_version: self.pricing_version.map(|v| v as u32),
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[async_trait]
impl SubscriptionRepository for PostgresSubscriptionRepository {
    async fn create(&self, subscription: &Subscription) -> RepoResult<()> {
        let row = SubscriptionRow::from_domain(subscription);
        sqlx::query(
            "INSERT INTO subscriptions \
             (id, tenant_id, plan_tier, status, trial_ends_at, grace_period_ends_at, \
              current_period_ends_at, canceled_at, cancel_at_period_end, \
              monitors, interval_seconds, retention_days, pricing_version, \
              created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(row.id)
        .bind(row.tenant_id)
        .bind(row.plan_tier)
        .bind(row.status)
        .bind(row.trial_ends_at)
        .bind(row.grace_period_ends_at)
        .bind(row.current_period_ends_at)
        .bind(row.canceled_at)
        .bind(row.cancel_at_period_end)
        .bind(row.monitors)
        .bind(row.interval_seconds)
        .bind(row.retention_days)
        .bind(row.pricing_version)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_tenant_id(&self, tenant_id: TenantId) -> RepoResult<Option<Subscription>> {
        let row = sqlx::query_as::<_, SubscriptionRow>(
            "SELECT id, tenant_id, plan_tier, status, trial_ends_at, grace_period_ends_at, \
             current_period_ends_at, canceled_at, cancel_at_period_end, \
             monitors, interval_seconds, retention_days, pricing_version, \
             created_at, updated_at \
             FROM subscriptions WHERE tenant_id = $1",
        )
        .bind(tenant_id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(SubscriptionRow::into_domain).transpose()
    }

    async fn find_test_plan_override(&self, tenant_id: TenantId) -> RepoResult<Option<PlanTier>> {
        let tier: Option<String> = sqlx::query_scalar(
            "SELECT plan_tier FROM test_access_grants \
             WHERE tenant_id = $1 AND enabled = TRUE \
               AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(tenant_id.0)
        .fetch_optional(&self.pool)
        .await?;
        tier.map(|value| {
            value
                .parse::<PlanTier>()
                .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
        })
        .transpose()
    }

    async fn update(&self, subscription: &Subscription) -> RepoResult<()> {
        let row = SubscriptionRow::from_domain(subscription);
        // SECURITY (defense-in-depth): every caller of `update` already
        // loads the subscription via `find_by_tenant_id(caller's own
        // tenant_id)` before mutating it, so `row.id` cannot currently be
        // attacker-controlled across tenants. The `AND tenant_id = $14`
        // predicate below is a second, independent guard at the SQL layer
        // so that a future caller that forgets that invariant still can't
        // update another tenant's subscription row.
        let result = sqlx::query(
            "UPDATE subscriptions SET \
             plan_tier = $2, status = $3, trial_ends_at = $4, grace_period_ends_at = $5, \
             current_period_ends_at = $6, canceled_at = $7, cancel_at_period_end = $8, \
             monitors = $9, interval_seconds = $10, retention_days = $11, pricing_version = $12, \
             updated_at = $13 \
             WHERE id = $1 \
               AND tenant_id = $14",
        )
        .bind(row.id)
        .bind(row.plan_tier)
        .bind(row.status)
        .bind(row.trial_ends_at)
        .bind(row.grace_period_ends_at)
        .bind(row.current_period_ends_at)
        .bind(row.canceled_at)
        .bind(row.cancel_at_period_end)
        .bind(row.monitors)
        .bind(row.interval_seconds)
        .bind(row.retention_days)
        .bind(row.pricing_version)
        .bind(row.updated_at)
        .bind(row.tenant_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err("subscription update affected no rows (id/tenant mismatch)".into());
        }

        Ok(())
    }
}
