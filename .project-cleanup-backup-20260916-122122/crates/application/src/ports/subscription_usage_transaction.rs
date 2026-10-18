use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

use domain::Subscription;
use domain::entities::SubscriptionUsagePeriod;
use domain::enums::PlanTier;
use domain::shared::ids::SubscriptionUsagePeriodId;

pub type SubscriptionUsageTransactionResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Represents one atomic (all-or-nothing) mid-cycle usage change (H12 —
/// Proration Service): closing whatever usage period is currently open
/// (if any), opening a new one with the new usage, and updating the
/// `subscriptions` row's current usage snapshot — all in a single DB
/// transaction. Application layer does not know this is `sqlx` under the
/// hood; that lives in `infra` only (same shape as `RegistrationTransaction`).
#[async_trait]
pub trait SubscriptionUsageTransaction: Send + Sync {
    /// `previous_open_period_id` is `None` the first time a subscription
    /// ever gets a usage period (e.g. right after checkout); `Some` on
    /// every subsequent mid-cycle change.
    async fn change_usage(
        &self,
        subscription: &Subscription,
        previous_open_period_id: Option<SubscriptionUsagePeriodId>,
        new_period: &SubscriptionUsagePeriod,
        expected_old_plan: Option<PlanTier>,
    ) -> SubscriptionUsageTransactionResult<()>;
}

#[async_trait]
impl<T> SubscriptionUsageTransaction for Arc<T>
where
    T: SubscriptionUsageTransaction + Send + Sync + ?Sized,
{
    async fn change_usage(
        &self,
        subscription: &Subscription,
        previous_open_period_id: Option<SubscriptionUsagePeriodId>,
        new_period: &SubscriptionUsagePeriod,
        expected_old_plan: Option<PlanTier>,
    ) -> SubscriptionUsageTransactionResult<()> {
        (**self)
            .change_usage(
                subscription,
                previous_open_period_id,
                new_period,
                expected_old_plan,
            )
            .await
    }
}
