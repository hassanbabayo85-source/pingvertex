use std::error::Error;

use crate::{ports::SubscriptionRepository, queries::ResolveEntitlements};
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
}

impl<R> ResolveEntitlementsUseCase<R>
where
    R: SubscriptionRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, query: ResolveEntitlements) -> UseCaseResult<Plan> {
        if let Some(test_tier) = self
            .repository
            .find_test_plan_override(query.tenant_id)
            .await?
        {
            return Ok(Plan::for_tier(test_tier));
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

        Ok(Plan::for_tier(plan_tier))
    }
}
