use serde::{Deserialize, Serialize};

/// Where a tenant's subscription currently sits in its lifecycle.
///
/// - `Trialing`: on a time-limited trial, full entitlements, no payment
///   attempted yet.
/// - `Active`: normal paid (or free-tier) subscription in good standing.
/// - `PastDue`: a payment failed/expired; the tenant is in a grace
///   period and keeps full entitlements until the grace period ends.
/// - `Expired`: the grace period (or an unconverted trial) ran out with
///   no successful payment; entitlements fall back to Free.
/// - `Canceled`: the tenant (or an admin) canceled the subscription,
///   either immediately or at the end of the current billing period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionStatus {
    Trialing,
    Active,
    PastDue,
    Expired,
    Canceled,
}

impl SubscriptionStatus {
    /// Stable machine-readable string, used as the `status` column value
    /// in the `subscriptions` table (see
    /// migrations/0039_alter_subscriptions_add_lifecycle.sql).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trialing => "trialing",
            Self::Active => "active",
            Self::PastDue => "past_due",
            Self::Expired => "expired",
            Self::Canceled => "canceled",
        }
    }
}

impl std::str::FromStr for SubscriptionStatus {
    type Err = crate::errors::DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "trialing" => Ok(Self::Trialing),
            "active" => Ok(Self::Active),
            "past_due" => Ok(Self::PastDue),
            "expired" => Ok(Self::Expired),
            "canceled" => Ok(Self::Canceled),
            other => Err(crate::errors::DomainError::InvalidSubscriptionStatus(
                other.to_owned(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_status_through_as_str_and_from_str() {
        for status in [
            SubscriptionStatus::Trialing,
            SubscriptionStatus::Active,
            SubscriptionStatus::PastDue,
            SubscriptionStatus::Expired,
            SubscriptionStatus::Canceled,
        ] {
            let parsed: SubscriptionStatus = status.as_str().parse().unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn rejects_unknown_status_string() {
        let result: Result<SubscriptionStatus, _> = "unknown".parse();
        assert_eq!(
            result,
            Err(crate::errors::DomainError::InvalidSubscriptionStatus(
                "unknown".to_owned()
            ))
        );
    }
}
