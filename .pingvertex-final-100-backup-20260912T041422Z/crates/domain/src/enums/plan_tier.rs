use serde::{Deserialize, Serialize};

/// Subscription capability tiers. Plan pricing is deliberately not hardcoded here;
/// limits are entitlements while monetary values come from the versioned pricing
/// configuration.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanTier {
    #[default]
    Free,
    Starter,
    Pro,
    Business,
    Enterprise,
}

impl PlanTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Starter => "starter",
            Self::Pro => "pro",
            Self::Business => "business",
            Self::Enterprise => "enterprise",
        }
    }
}

impl std::str::FromStr for PlanTier {
    type Err = crate::errors::DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "free" => Ok(Self::Free),
            "starter" => Ok(Self::Starter),
            "pro" => Ok(Self::Pro),
            "business" => Ok(Self::Business),
            "enterprise" => Ok(Self::Enterprise),
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
    fn round_trips_all_tiers() {
        for tier in [
            PlanTier::Free,
            PlanTier::Starter,
            PlanTier::Pro,
            PlanTier::Business,
            PlanTier::Enterprise,
        ] {
            assert_eq!(tier.as_str().parse::<PlanTier>().unwrap(), tier);
        }
    }

    #[test]
    fn rejects_unknown_tier_string() {
        let result: Result<PlanTier, _> = "unknown".parse();
        assert!(result.is_err());
    }
}
