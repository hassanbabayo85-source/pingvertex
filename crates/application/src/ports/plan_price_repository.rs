//! Read side of the versioned `plan_prices` table.
//!
//! `PlanPriceRepository` is the ONLY authorized source of a plan's
//! server-side price. Checkout use cases must never trust an amount or
//! currency supplied by the client; they call `find_effective` with the
//! requested (tier, currency, interval) and use the returned amount.

use async_trait::async_trait;

use domain::PlanTier;
use domain::entities::PlanPrice;
use domain::money::{BillingInterval, Currency};

pub type PlanPriceRepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[async_trait]
pub trait PlanPriceRepository: Send + Sync {
    /// Returns the currently effective price for `(tier, currency, interval)`.
    ///
    /// Selection rule: highest `version` among rows whose
    /// `effective_from <= now()` and (`effective_until IS NULL` or
    /// `effective_until > now()`). Enterprise has no seeded prices and
    /// returns `None`.
    async fn find_effective(
        &self,
        plan_tier: PlanTier,
        currency: Currency,
        billing_interval: BillingInterval,
    ) -> PlanPriceRepoResult<Option<PlanPrice>>;
}

#[async_trait]
impl<T> PlanPriceRepository for std::sync::Arc<T>
where
    T: PlanPriceRepository + ?Sized,
{
    async fn find_effective(
        &self,
        plan_tier: PlanTier,
        currency: Currency,
        billing_interval: BillingInterval,
    ) -> PlanPriceRepoResult<Option<PlanPrice>> {
        (**self)
            .find_effective(plan_tier, currency, billing_interval)
            .await
    }
}
