use std::error::Error;

use application::ports::SubscriptionUsagePeriodRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::SubscriptionUsagePeriod;
use domain::shared::ids::{SubscriptionId, SubscriptionUsagePeriodId};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresSubscriptionUsagePeriodRepository {
    pool: PgPool,
}

impl PostgresSubscriptionUsagePeriodRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct SubscriptionUsagePeriodRow {
    id: Uuid,
    subscription_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: Option<DateTime<Utc>>,
    monitors: i64,
    interval_seconds: i32,
    retention_days: i32,
    created_at: DateTime<Utc>,
}

impl SubscriptionUsagePeriodRow {
    fn into_domain(self) -> SubscriptionUsagePeriod {
        SubscriptionUsagePeriod {
            id: SubscriptionUsagePeriodId(self.id),
            subscription_id: SubscriptionId(self.subscription_id),
            start_at: self.start_at,
            end_at: self.end_at,
            monitors: self.monitors as u64,
            interval_seconds: self.interval_seconds as u32,
            retention_days: self.retention_days as u32,
            created_at: self.created_at,
        }
    }
}

#[async_trait]
impl SubscriptionUsagePeriodRepository for PostgresSubscriptionUsagePeriodRepository {
    async fn create_period(&self, period: &SubscriptionUsagePeriod) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO subscription_usage_periods \
             (id, subscription_id, start_at, end_at, monitors, interval_seconds, retention_days, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(period.id.0)
        .bind(period.subscription_id.0)
        .bind(period.start_at)
        .bind(period.end_at)
        .bind(period.monitors as i64)
        .bind(period.interval_seconds as i32)
        .bind(period.retention_days as i32)
        .bind(period.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn close_period(
        &self,
        period_id: SubscriptionUsagePeriodId,
        end_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        sqlx::query("UPDATE subscription_usage_periods SET end_at = $2 WHERE id = $1")
            .bind(period_id.0)
            .bind(end_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn find_open_period(
        &self,
        subscription_id: SubscriptionId,
    ) -> RepoResult<Option<SubscriptionUsagePeriod>> {
        let row = sqlx::query_as::<_, SubscriptionUsagePeriodRow>(
            "SELECT id, subscription_id, start_at, end_at, monitors, interval_seconds, retention_days, created_at \
             FROM subscription_usage_periods \
             WHERE subscription_id = $1 AND end_at IS NULL \
             ORDER BY start_at DESC LIMIT 1",
        )
        .bind(subscription_id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(SubscriptionUsagePeriodRow::into_domain))
    }

    async fn list_periods_for_cycle(
        &self,
        subscription_id: SubscriptionId,
        cycle_start: DateTime<Utc>,
        cycle_end: DateTime<Utc>,
    ) -> RepoResult<Vec<SubscriptionUsagePeriod>> {
        let rows = sqlx::query_as::<_, SubscriptionUsagePeriodRow>(
            "SELECT id, subscription_id, start_at, end_at, monitors, interval_seconds, retention_days, created_at \
             FROM subscription_usage_periods \
             WHERE subscription_id = $1 \
               AND start_at < $3 \
               AND (end_at IS NULL OR end_at > $2) \
             ORDER BY start_at ASC",
        )
        .bind(subscription_id.0)
        .bind(cycle_start)
        .bind(cycle_end)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(SubscriptionUsagePeriodRow::into_domain)
            .collect())
    }
}
