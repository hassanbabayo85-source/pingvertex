use std::error::Error;

use crate::{
    ports::{PricingConfigRepository, SubscriptionRepository},
    queries::ResolveEntitlements,
};
use domain::{Plan, enums::PlanTier, enums::SubscriptionStatus};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// **LEGACY — do not use in new code.**
///
/// `ResolveEntitlementsUseCase` resolves subscription-based plan limits
/// (`Free` / `Starter` / `Pro` / `Business` / `Enterprise`) from a
/// `SubscriptionRepository` row.
///
/// As of the PAYG runtime, this resolver has **no production caller**:
///
/// - The durable scheduler resolves free-tier entitlements directly
///   from `PricingConfigRepository` (`durable_scheduler.rs:281`).
/// - `RetentionWorker` is PAYG / pricing-config driven and does not
///   depend on subscription state.
/// - API handlers use PAYG wallet/pricing paths, not this use case.
///
/// It is kept only so subscription-related tests and migration work can
/// continue to exercise the historical `SubscriptionStatus → PlanTier`
/// mapping. **New runtime access decisions MUST NOT depend on this
/// resolver or on `SubscriptionRepository`.**
///
/// Historical behavior (for the benefit of those tests): a tenant with
/// no `Subscription` row is treated as Free. `Trialing`, `Active`, and
/// `PastDue` subscriptions resolve full entitlements for their tier
/// (grace period is a soft landing, not an immediate cutoff);
/// `Expired` and `Canceled` fall back to Free.
#[doc(hidden)]
pub struct ResolveEntitlementsUseCase<R> {
    repository: R,
    pricing_repository: Option<std::sync::Arc<dyn PricingConfigRepository>>,
}

impl<R> ResolveEntitlementsUseCase<R>
where
    R: SubscriptionRepository,
{
    pub fn new(repository: R) -> Self {
        Self {
            repository,
            pricing_repository: None,
        }
    }

    pub fn with_pricing_config_repository(
        mut self,
        repository: std::sync::Arc<dyn PricingConfigRepository>,
    ) -> Self {
        self.pricing_repository = Some(repository);
        self
    }

    pub async fn execute(&self, query: ResolveEntitlements) -> UseCaseResult<Plan> {
        if let Some(test_tier) = self
            .repository
            .find_test_plan_override(query.tenant_id)
            .await?
        {
            let mut plan = Plan::for_tier(test_tier);
            if test_tier == PlanTier::Free
                && let Some(pricing_repository) = &self.pricing_repository
                && let Some(config) = pricing_repository.find_active_config().await?
            {
                plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
                plan.min_check_interval_seconds = config.free_tier.interval_seconds;
                plan.history_retention_days = config.free_tier.retention_days;
            }
            return Ok(plan);
        }

        let plan_tier = self
            .repository
            .find_by_tenant_id(query.tenant_id)
            .await?
            .map(|subscription| match subscription.status {
                SubscriptionStatus::Expired | SubscriptionStatus::Canceled => PlanTier::Free,
                SubscriptionStatus::Trialing
                | SubscriptionStatus::Active
                | SubscriptionStatus::PastDue => subscription.plan_tier,
            })
            .unwrap_or(PlanTier::Free);

        let mut plan = Plan::for_tier(plan_tier);
        if plan_tier == PlanTier::Free
            && let Some(pricing_repository) = &self.pricing_repository
            && let Some(config) = pricing_repository.find_active_config().await?
        {
            plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
            plan.min_check_interval_seconds = config.free_tier.interval_seconds;
            plan.history_retention_days = config.free_tier.retention_days;
        }
        Ok(plan)
    }
}
