use std::error::Error;

use application::ports::SubscriptionUsageTransaction;
use async_trait::async_trait;
use domain::Subscription;
use domain::entities::SubscriptionUsagePeriod;
use domain::shared::ids::SubscriptionUsagePeriodId;
use sqlx::PgPool;

type TxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Postgres implementation of `SubscriptionUsageTransaction` (H12 —
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
        // SECURITY (defense-in-depth): `subscription` is always the row
        // `UpdateSubscriptionUsageUseCase` loaded via
        // `find_by_tenant_id(caller's own tenant_id)`, so `subscription.id`
        // cannot currently be attacker-controlled across tenants. The
        // `AND tenant_id = $7` predicate is a second, independent guard at
        // the SQL layer, and `rows_affected() == 0` is treated as a hard
        // error rather than a silent no-op so a tenant mismatch can never
        // pass as success.
        let result = sqlx::query(
            "UPDATE subscriptions SET \
             monitors = $2, interval_seconds = $3, retention_days = $4, pricing_version = $5, updated_at = $6 \
             WHERE id = $1 \
               AND tenant_id = $7",
        )
        .bind(subscription.id.0)
        .bind(subscription.monitors.map(|m| m as i64))
        .bind(subscription.interval_seconds.map(|i| i as i32))
        .bind(subscription.retention_days.map(|r| r as i32))
        .bind(subscription.pricing_version.map(|v| v as i32))
        .bind(subscription.updated_at)
        .bind(subscription.tenant_id.0)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            return Err("subscription usage update affected no rows (id/tenant mismatch)".into());
        }

        tx.commit().await?;

        Ok(())
    }
}
