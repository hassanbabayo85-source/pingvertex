use std::error::Error;
use std::sync::Arc;

use application::ports::{
    SubscriptionRepository, SubscriptionUsagePeriodRepository, SubscriptionUsageTransaction,
};
use async_trait::async_trait;
use domain::Subscription;
use domain::entities::SubscriptionUsagePeriod;
use domain::enums::PlanTier;
use domain::shared::ids::SubscriptionUsagePeriodId;

/// Test-only in-memory stand-in for `SubscriptionUsageTransaction`.
///
/// Unlike `InMemoryRegistrationTransaction` (a pure no-op used only to
/// satisfy HTTP-layer test wiring), this one actually delegates to the
/// in-memory repositories it wraps so `UpdateSubscriptionUsageUseCase`
/// can be exercised end-to-end in application-layer tests without a real
/// database. It does NOT provide atomicity/rollback — only
/// `PostgresSubscriptionUsageTransaction` does; real atomicity is only
/// verified against a real Postgres instance (see
/// `crates/infra/tests/subscription_usage_transaction.rs`).
#[derive(Clone)]
pub struct InMemorySubscriptionUsageTransaction {
    subscription_repository: Arc<dyn SubscriptionRepository>,
    usage_period_repository: Arc<dyn SubscriptionUsagePeriodRepository>,
}

impl InMemorySubscriptionUsageTransaction {
    pub fn new(
        subscription_repository: Arc<dyn SubscriptionRepository>,
        usage_period_repository: Arc<dyn SubscriptionUsagePeriodRepository>,
    ) -> Self {
        Self {
            subscription_repository,
            usage_period_repository,
        }
    }
}

#[async_trait]
impl SubscriptionUsageTransaction for InMemorySubscriptionUsageTransaction {
    async fn change_usage(
        &self,
        subscription: &Subscription,
        previous_open_period_id: Option<SubscriptionUsagePeriodId>,
        new_period: &SubscriptionUsagePeriod,
        expected_old_plan: Option<PlanTier>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(expected) = expected_old_plan {
            let current = self
                .subscription_repository
                .find_by_tenant_id(subscription.tenant_id)
                .await?
                .ok_or("subscription not found")?;

            if current.plan_tier != expected {
                return Err("subscription plan changed concurrently".into());
            }
        }

        if let Some(period_id) = previous_open_period_id {
            self.usage_period_repository
                .close_period(period_id, new_period.start_at)
                .await?;
        }
        self.usage_period_repository
            .create_period(new_period)
            .await?;
        self.subscription_repository.update(subscription).await?;
        Ok(())
    }
}
