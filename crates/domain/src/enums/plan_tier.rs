use serde::{Deserialize, Serialize};

/// Which of PingVertex's five fixed subscription plans a tenant is on.
///
/// This is a closed, fixed-cardinality set — not a database table —
/// because the plan catalog itself (limits, pricing) is a business rule
/// baked into this code, not tenant-editable data. See `Plan::for_tier`
/// for the actual limits attached to each tier.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanTier {
    #[default]
    Free,
    Tier1,
    Tier5,
    Tier10,
    Tier20,
}

impl PlanTier {
    /// Stable machine-readable string, used as the `plan_tier` column value
    /// in the `subscriptions` table (see
    /// migrations/0038_create_subscriptions.sql) and for parsing that
    /// column back via `from_str`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Tier1 => "tier_1",
            Self::Tier5 => "tier_5",
            Self::Tier10 => "tier_10",
            Self::Tier20 => "tier_20",
        }
    }
}

impl std::str::FromStr for PlanTier {
    type Err = crate::errors::DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "free" => Ok(Self::Free),
            "tier_1" => Ok(Self::Tier1),
            "tier_5" => Ok(Self::Tier5),
            "tier_10" => Ok(Self::Tier10),
            "tier_20" => Ok(Self::Tier20),
            other => Err(crate::errors::DomainError::InvalidPlanTier(
                other.to_owned(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_tier_through_as_str_and_from_str() {
        for tier in [
            PlanTier::Free,
            PlanTier::Tier1,
            PlanTier::Tier5,
            PlanTier::Tier10,
            PlanTier::Tier20,
        ] {
            let parsed: PlanTier = tier.as_str().parse().unwrap();
            assert_eq!(parsed, tier);
        }
    }

    #[test]
    fn rejects_unknown_tier_string() {
        let result: Result<PlanTier, _> = "unknown".parse();
        assert_eq!(
            result,
            Err(crate::errors::DomainError::InvalidPlanTier(
                "unknown".to_owned()
            ))
        );
    }
}
