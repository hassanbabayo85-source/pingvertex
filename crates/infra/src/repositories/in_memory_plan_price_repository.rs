use application::ports::{PlanPriceRepoResult, PlanPriceRepository};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use domain::PlanTier;
use domain::entities::PlanPrice;
use domain::money::{BillingInterval, Currency, Money};
use uuid::Uuid;

/// In-memory `PlanPriceRepository` seeded with the exact v1 prices
/// inserted by migration 0125 (Free/Starter/Pro/Business, NGN+USD,
/// monthly+annual). Enterprise has no seeded prices and is absent.
pub struct InMemoryPlanPriceRepository {
    prices: Vec<PlanPrice>,
}

impl Default for InMemoryPlanPriceRepository {
    fn default() -> Self {
        Self::with_default_seed()
    }
}

impl InMemoryPlanPriceRepository {
    pub fn new(prices: Vec<PlanPrice>) -> Self {
        Self { prices }
    }

    pub fn with_default_seed() -> Self {
        let effective_from = Utc::now() - Duration::days(1);
        let mut prices = Vec::new();

        let rows: [(PlanTier, &str, i64, i64, i64, i64); 4] = [
            // (tier, plan_uuid, usd_monthly, usd_annual, ngn_monthly, ngn_annual)
            (PlanTier::Free, "00000000-0000-0000-0000-0000000000a1", 0, 0, 0, 0),
            (PlanTier::Starter, "00000000-0000-0000-0000-0000000000a2", 500, 5_000, 800_000, 8_000_000),
            (PlanTier::Pro, "00000000-0000-0000-0000-0000000000a3", 1_900, 19_000, 3_000_000, 30_000_000),
            (PlanTier::Business, "00000000-0000-0000-0000-0000000000a4", 4_900, 49_000, 7_700_000, 77_000_000),
        ];

        for (tier, plan_id, usd_m, usd_a, ngn_m, ngn_a) in rows {
            let plan_id = Uuid::parse_str(plan_id).expect("seed plan uuid");
            for (currency, monthly, annual) in [
                (Currency::Usd, usd_m, usd_a),
                (Currency::Ngn, ngn_m, ngn_a),
            ] {
                for (interval, amount_minor) in [
                    (BillingInterval::Monthly, monthly),
                    (BillingInterval::Annual, annual),
                ] {
                    prices.push(
                        PlanPrice::new(
                            Uuid::new_v4(),
                            plan_id,
                            currency,
                            interval,
                            Money::new(amount_minor, currency).expect("seed amount"),
                            1,
                            effective_from,
                        )
                        .expect("seed plan price"),
                    );
                }
            }
            let _ = tier;
        }

        Self { prices }
    }
}

#[async_trait]
impl PlanPriceRepository for InMemoryPlanPriceRepository {
    async fn find_effective(
        &self,
        plan_tier: PlanTier,
        currency: Currency,
        billing_interval: BillingInterval,
    ) -> PlanPriceRepoResult<Option<PlanPrice>> {
        let now = Utc::now();
        let mut candidates: Vec<&PlanPrice> = self
            .prices
            .iter()
            .filter(|p| {
                p.currency == currency
                    && p.billing_interval == billing_interval
                    && p.effective_from <= now
                    && p.effective_until.is_none_or(|until| until > now)
            })
            .collect();

        // PlanId → tier is not carried on PlanPrice, so resolve via the
        // seeded plan_uuid prefix. Instead of coupling to UUIDs, seed
        // order is stable: each tier owns exactly 4 rows. We select by
        // matching the plan_id recorded in the seed table.
        let tier_plan = match plan_tier {
            PlanTier::Free => "00000000-0000-0000-0000-0000000000a1",
            PlanTier::Starter => "00000000-0000-0000-0000-0000000000a2",
            PlanTier::Pro => "00000000-0000-0000-0000-0000000000a3",
            PlanTier::Business => "00000000-0000-0000-0000-0000000000a4",
            PlanTier::Enterprise => return Ok(None),
        };
        let tier_plan = Uuid::parse_str(tier_plan).expect("seed plan uuid");

        candidates.retain(|p| p.plan_id == tier_plan);
        candidates.sort_by(|a, b| b.version.cmp(&a.version));
        Ok(candidates.first().map(|p| (*p).clone()))
    }
}
