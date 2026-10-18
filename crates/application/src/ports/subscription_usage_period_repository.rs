use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use chrono::{DateTime, Utc};
use domain::entities::SubscriptionUsagePeriod;
use domain::shared::ids::{SubscriptionId, SubscriptionUsagePeriodId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for `SubscriptionUsagePeriod` (Proration
/// Service).
///
/// Deliberately has NO generic `update`/`delete`: a period is either
/// created open (`end_at = NULL`) or closed via `close_period` (setting
/// `end_at`) — there is no other mutation, and periods are never deleted.
#[async_trait]
pub trait SubscriptionUsagePeriodRepository: Send + Sync {
    async fn create_period(&self, period: &SubscriptionUsagePeriod) -> RepoResult<()>;

    /// Closes the given period by setting `end_at`.
    async fn close_period(
        &self,
        period_id: SubscriptionUsagePeriodId,
        end_at: DateTime<Utc>,
    ) -> RepoResult<()>;

    /// Returns the currently-open period (`end_at IS NULL`) for a
    /// subscription, if any. A subscription should have at most one open
    /// period at a time.
    async fn find_open_period(
        &self,
        subscription_id: SubscriptionId,
    ) -> RepoResult<Option<SubscriptionUsagePeriod>>;

    /// Returns every period that overlaps `[cycle_start, cycle_end)` for
    /// a subscription, ordered by `start_at` ascending — used by billing
    /// to sum `calculate_period_cost` across every period touched during
    /// one billing cycle.
    async fn list_periods_for_cycle(
        &self,
        subscription_id: SubscriptionId,
        cycle_start: DateTime<Utc>,
        cycle_end: DateTime<Utc>,
    ) -> RepoResult<Vec<SubscriptionUsagePeriod>>;
}

#[async_trait]
impl<T> SubscriptionUsagePeriodRepository for Arc<T>
where
    T: SubscriptionUsagePeriodRepository + Send + Sync + ?Sized,
{
    async fn create_period(&self, period: &SubscriptionUsagePeriod) -> RepoResult<()> {
        (**self).create_period(period).await
    }

    async fn close_period(
        &self,
        period_id: SubscriptionUsagePeriodId,
        end_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        (**self).close_period(period_id, end_at).await
    }

    async fn find_open_period(
        &self,
        subscription_id: SubscriptionId,
    ) -> RepoResult<Option<SubscriptionUsagePeriod>> {
        (**self).find_open_period(subscription_id).await
    }

    async fn list_periods_for_cycle(
        &self,
        subscription_id: SubscriptionId,
        cycle_start: DateTime<Utc>,
        cycle_end: DateTime<Utc>,
    ) -> RepoResult<Vec<SubscriptionUsagePeriod>> {
        (**self)
            .list_periods_for_cycle(subscription_id, cycle_start, cycle_end)
            .await
    }
}
