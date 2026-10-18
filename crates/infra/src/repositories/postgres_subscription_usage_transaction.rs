use std::error::Error;

use application::ports::SubscriptionUsageTransaction;
use async_trait::async_trait;
use domain::Subscription;
use domain::entities::SubscriptionUsagePeriod;
use domain::shared::ids::SubscriptionUsagePeriodId;
use sqlx::PgPool;

type TxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Postgres implementation of `SubscriptionUsageTransaction` (
/// Proration Service).
///
/// Closes the previously-open `subscription_usage_periods` row (if any),
/// inserts the new one, and updates the `subscriptions` row's usage
/// snapshot columns — all in a single `sqlx::Transaction`, matching
/// `PostgresRegistrationTransaction`'s shape: if any statement fails,
/// `?` returns before `tx.commit()` and sqlx rolls back automatically
/// when `tx` drops.
#[derive(Debug, Clone)]
pub struct PostgresSubscriptionUsageTransaction {
    pool: PgPool,
}

impl PostgresSubscriptionUsageTransaction {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SubscriptionUsageTransaction for PostgresSubscriptionUsageTransaction {
    async fn change_usage(
        &self,
        subscription: &Subscription,
        previous_open_period_id: Option<SubscriptionUsagePeriodId>,
        new_period: &SubscriptionUsagePeriod,
        expected_old_plan: Option<domain::enums::PlanTier>,
    ) -> TxResult<()> {
        let mut tx = self.pool.begin().await?;

        // 1. Close whatever period was open, if any.
        if let Some(period_id) = previous_open_period_id {
            sqlx::query("UPDATE subscription_usage_periods SET end_at = $2 WHERE id = $1")
                .bind(period_id.0)
                .bind(new_period.start_at)
                .execute(&mut *tx)
                .await?;
        }

        // 2. Open the new period.
        sqlx::query(
            "INSERT INTO subscription_usage_periods \
             (id, subscription_id, start_at, end_at, monitors, interval_seconds, retention_days, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(new_period.id.0)
        .bind(new_period.subscription_id.0)
        .bind(new_period.start_at)
        .bind(new_period.end_at)
        .bind(new_period.monitors as i64)
        .bind(new_period.interval_seconds as i32)
        .bind(new_period.retention_days as i32)
        .bind(new_period.created_at)
        .execute(&mut *tx)
        .await?;

        // 3. Update the subscription's current usage snapshot.
        //
        // SECURITY + CONCURRENCY: when a caller supplies the plan it
        // originally observed, require that plan to still be current.
        // Because this UPDATE is inside the same transaction that closes
        // the old usage period and inserts the new one, a failed CAS causes
        // the entire transaction to roll back.
        let mut query = match expected_old_plan {
            Some(_) => sqlx::query(
                "UPDATE subscriptions SET                  plan_tier = $2, status = $3, trial_ends_at = $4, grace_period_ends_at = $5, current_period_ends_at = $6, canceled_at = $7, cancel_at_period_end = $8, monitors = $9, interval_seconds = $10, retention_days = $11, pricing_version = $12, updated_at = $13                  WHERE id = $1 AND tenant_id = $14 AND plan_tier = $15",
            ),
            None => sqlx::query(
                "UPDATE subscriptions SET                  plan_tier = $2, status = $3, trial_ends_at = $4, grace_period_ends_at = $5, current_period_ends_at = $6, canceled_at = $7, cancel_at_period_end = $8, monitors = $9, interval_seconds = $10, retention_days = $11, pricing_version = $12, updated_at = $13                  WHERE id = $1 AND tenant_id = $14",
            ),
        };

        query = query
            .bind(subscription.id.0)
            .bind(subscription.plan_tier.as_str())
            .bind(subscription.status.as_str())
            .bind(subscription.trial_ends_at)
            .bind(subscription.grace_period_ends_at)
            .bind(subscription.current_period_ends_at)
            .bind(subscription.canceled_at)
            .bind(subscription.cancel_at_period_end)
            .bind(subscription.monitors.map(|m| m as i64))
            .bind(subscription.interval_seconds.map(|i| i as i32))
            .bind(subscription.retention_days.map(|r| r as i32))
            .bind(subscription.pricing_version.map(|v| v as i32))
            .bind(subscription.updated_at)
            .bind(subscription.tenant_id.0);

        if let Some(expected) = expected_old_plan {
            query = query.bind(expected.as_str());
        }

        let result = query.execute(&mut *tx).await?;

        if result.rows_affected() == 0 {
            if expected_old_plan.is_some() {
                return Err(
                    "subscription usage update affected no rows (id/tenant/expected-plan mismatch)"
                        .into(),
                );
            }

            return Err("subscription usage update affected no rows (id/tenant mismatch)".into());
        }

        tx.commit().await?;

        Ok(())
    }
}
