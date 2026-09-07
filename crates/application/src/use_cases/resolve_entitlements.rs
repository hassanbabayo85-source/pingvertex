use std::error::Error;

use crate::{
    ports::{PricingConfigRepository, SubscriptionRepository},
    queries::ResolveEntitlements,
};
use domain::{Plan, enums::PlanTier, enums::SubscriptionStatus};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Resolves the current entitlements (plan limits) for a tenant.
///
/// A reusable service, not tied to any HTTP handler, so later workers
/// (quota enforcement at resource creation, billing) can all call the
/// same resolution logic instead of re-deriving it.
///
/// A tenant with no `Subscription` row is treated as being on the Free
/// plan. As of B12, a subscription's *status* also affects resolution:
/// `Trialing`, `Active`, and `PastDue` (grace period) all resolve full
/// entitlements for the subscribed tier — grace period is a soft
/// landing, not an immediate cutoff. `Expired` and `Canceled`
/// subscriptions fall back to Free, same as having no row at all.
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
            if test_tier == PlanTier::Free {
                if let Some(pricing_repository) = &self.pricing_repository {
                    if let Some(config) = pricing_repository.find_active_config().await? {
                        plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
                        plan.min_check_interval_seconds = config.free_tier.interval_seconds;
                        plan.history_retention_days = config.free_tier.retention_days;
                    }
                }
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
        if plan_tier == PlanTier::Free {
            if let Some(pricing_repository) = &self.pricing_repository {
                if let Some(config) = pricing_repository.find_active_config().await? {
                    plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
                    plan.min_check_interval_seconds = config.free_tier.interval_seconds;
                    plan.history_retention_days = config.free_tier.retention_days;
                }
            }
        }
        Ok(plan)
    }
}
