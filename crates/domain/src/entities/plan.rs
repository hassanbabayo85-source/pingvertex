use crate::enums::PlanTier;

/// Capability/entitlement catalog. Monetary pricing is not stored here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub tier: PlanTier,
    pub max_monitors: u32,
    pub min_check_interval_seconds: u32,
    pub history_retention_days: u32,
    pub max_locations: u32,
    pub max_team_members: u32,
    pub rum_enabled: bool,
    pub advanced_features: bool,
}

impl Plan {
    pub fn for_tier(tier: PlanTier) -> Self {
        match tier {
            PlanTier::Free => Self {
                tier,
                max_monitors: 10,
                min_check_interval_seconds: 300,
                history_retention_days: 2,
                max_locations: 1,
                max_team_members: 1,
                rum_enabled: false,
                advanced_features: false,
            },
            PlanTier::Starter => Self {
                tier,
                max_monitors: 25,
                min_check_interval_seconds: 60,
                history_retention_days: 30,
                max_locations: 1,
                max_team_members: 2,
                rum_enabled: false,
                advanced_features: false,
            },
            PlanTier::Pro => Self {
                tier,
                max_monitors: 100,
                min_check_interval_seconds: 30,
                history_retention_days: 90,
                max_locations: 3,
                max_team_members: 10,
                rum_enabled: true,
                advanced_features: true,
            },
            PlanTier::Business => Self {
                tier,
                max_monitors: 500,
                min_check_interval_seconds: 15,
                history_retention_days: 365,
                max_locations: 10,
                max_team_members: 50,
                rum_enabled: true,
                advanced_features: true,
            },
            PlanTier::Enterprise => Self {
                tier,
                max_monitors: 10_000,
                min_check_interval_seconds: 15,
                history_retention_days: 3650,
                max_locations: 100,
                max_team_members: 1000,
                rum_enabled: true,
                advanced_features: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_plan_matches_product_entitlements() {
        let p = Plan::for_tier(PlanTier::Free);
        assert_eq!(p.max_monitors, 10);
        assert_eq!(p.min_check_interval_seconds, 300);
        assert_eq!(p.history_retention_days, 2);
        assert_eq!(p.max_locations, 1);
        assert_eq!(p.max_team_members, 1);
        assert!(!p.rum_enabled);
        assert!(!p.advanced_features);
    }
}
