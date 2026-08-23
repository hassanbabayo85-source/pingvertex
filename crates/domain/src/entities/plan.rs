use crate::enums::PlanTier;

/// The fixed limits/features attached to a `PlanTier`.
///
/// This is a read-only, in-memory catalog — not a database-backed
/// entity. There is deliberately no `plans` table: the five plans and
/// their limits are business rules baked into this code, not
/// tenant-editable data. Only which tier a given tenant is *subscribed
/// to* is persisted (see `Subscription` +
/// migrations/0038_create_subscriptions.sql).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub tier: PlanTier,
    pub max_monitors: u32,
    pub min_check_interval_seconds: u32,
    pub history_retention_days: u32,
}

impl Plan {
    /// Returns the fixed limits for a given plan tier.
    pub fn for_tier(tier: PlanTier) -> Self {
        match tier {
            PlanTier::Free => Self {
                tier,
                max_monitors: 100,
                min_check_interval_seconds: 120,
                history_retention_days: 7,
            },
            PlanTier::Tier1 => Self {
                tier,
                max_monitors: 250,
                min_check_interval_seconds: 60,
                history_retention_days: 30,
            },
            PlanTier::Tier5 => Self {
                tier,
                max_monitors: 500,
                min_check_interval_seconds: 60,
                history_retention_days: 90,
            },
            PlanTier::Tier10 => Self {
                tier,
                max_monitors: 1000,
                min_check_interval_seconds: 30,
                history_retention_days: 180,
            },
            PlanTier::Tier20 => Self {
                tier,
                max_monitors: 2500,
                min_check_interval_seconds: 30,
                history_retention_days: 365,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_plan_has_correct_limits() {
        let plan = Plan::for_tier(PlanTier::Free);
        assert_eq!(plan.max_monitors, 100);
        assert_eq!(plan.min_check_interval_seconds, 120);
        assert_eq!(plan.history_retention_days, 7);
    }

    #[test]
    fn tier1_plan_has_correct_limits() {
        let plan = Plan::for_tier(PlanTier::Tier1);
        assert_eq!(plan.max_monitors, 250);
        assert_eq!(plan.min_check_interval_seconds, 60);
        assert_eq!(plan.history_retention_days, 30);
    }

    #[test]
    fn tier5_plan_has_correct_limits() {
        let plan = Plan::for_tier(PlanTier::Tier5);
        assert_eq!(plan.max_monitors, 500);
        assert_eq!(plan.min_check_interval_seconds, 60);
        assert_eq!(plan.history_retention_days, 90);
    }

    #[test]
    fn tier10_plan_has_correct_limits() {
        let plan = Plan::for_tier(PlanTier::Tier10);
        assert_eq!(plan.max_monitors, 1000);
        assert_eq!(plan.min_check_interval_seconds, 30);
        assert_eq!(plan.history_retention_days, 180);
    }

    #[test]
    fn tier20_plan_has_correct_limits() {
        let plan = Plan::for_tier(PlanTier::Tier20);
        assert_eq!(plan.max_monitors, 2500);
        assert_eq!(plan.min_check_interval_seconds, 30);
        assert_eq!(plan.history_retention_days, 365);
    }
}
