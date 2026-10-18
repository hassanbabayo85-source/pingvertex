use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{SubscriptionId, SubscriptionUsagePeriodId};

/// A single window of time during which a subscription had one fixed
/// usage configuration (monitors/interval_seconds/retention_days).
///
/// Proration Service. A subscription accumulates one of these
/// every time its usage changes mid-cycle: the previous period is
/// closed (`end_at` set) and a new one is opened (`end_at = None`) with
/// the new usage. Billing sums the prorated cost of every period that
/// overlaps a billing cycle instead of charging a single flat rate for
/// whatever usage happens to be current when billing runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionUsagePeriod {
    pub id: SubscriptionUsagePeriodId,
    pub subscription_id: SubscriptionId,
    pub start_at: DateTime<Utc>,
    /// `None` while this period is the current, still-open one.
    pub end_at: Option<DateTime<Utc>>,
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
    pub created_at: DateTime<Utc>,
}

impl SubscriptionUsagePeriod {
    /// Opens a new, currently-active usage period starting at `start_at`.
    pub fn open(
        id: SubscriptionUsagePeriodId,
        subscription_id: SubscriptionId,
        monitors: u64,
        interval_seconds: u32,
        retention_days: u32,
        start_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            subscription_id,
            start_at,
            end_at: None,
            monitors,
            interval_seconds,
            retention_days,
            created_at: start_at,
        }
    }

    /// Closes this period at `end_at`. Rejects closing an
    /// already-closed period and rejects an `end_at` at or before
    /// `start_at` (a zero-or-negative-length period cannot be billed).
    pub fn close(&mut self, end_at: DateTime<Utc>) -> Result<(), DomainError> {
        if self.end_at.is_some() {
            return Err(DomainError::UsagePeriodAlreadyClosed);
        }
        if end_at <= self.start_at {
            return Err(DomainError::UsagePeriodEndBeforeStart);
        }
        self.end_at = Some(end_at);
        Ok(())
    }

    /// `true` while this period has no `end_at` (it is the subscription's
    /// current usage).
    pub fn is_open(&self) -> bool {
        self.end_at.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_period() -> SubscriptionUsagePeriod {
        SubscriptionUsagePeriod::open(
            SubscriptionUsagePeriodId::new(),
            SubscriptionId::new(),
            100,
            60,
            30,
            Utc::now(),
        )
    }

    #[test]
    fn open_creates_a_period_with_no_end_at() {
        let period = open_period();
        assert!(period.is_open());
        assert_eq!(period.end_at, None);
    }

    #[test]
    fn close_sets_end_at_and_closes_the_period() {
        let mut period = open_period();
        let end = period.start_at + chrono::Duration::days(10);
        period.close(end).unwrap();
        assert!(!period.is_open());
        assert_eq!(period.end_at, Some(end));
    }

    #[test]
    fn close_rejects_an_already_closed_period() {
        let mut period = open_period();
        let end = period.start_at + chrono::Duration::days(10);
        period.close(end).unwrap();
        let result = period.close(end + chrono::Duration::days(1));
        assert_eq!(result, Err(DomainError::UsagePeriodAlreadyClosed));
    }

    #[test]
    fn close_rejects_end_at_before_or_equal_to_start_at() {
        let mut period = open_period();
        let result = period.close(period.start_at);
        assert_eq!(result, Err(DomainError::UsagePeriodEndBeforeStart));
    }
}
