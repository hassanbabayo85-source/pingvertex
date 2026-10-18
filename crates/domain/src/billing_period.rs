//! Calendar-aware billing period arithmetic.
//!
//! Monthly and annual periods are CALENDAR periods, not fixed day
//! counts. A monthly period starting 2026-01-31 ends 2026-02-28 (or
//! 02-29 in a leap year); an annual period starting 2024-02-29 ends
//! 2025-02-28. `chrono::Months` performs exactly this clamping.

use chrono::{DateTime, Months, Utc};

use crate::money::BillingInterval;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BillingPeriodError {
    #[error("billing period overflow")]
    Overflow,
}

/// Returns `start + interval.months()` with calendar semantics.
pub fn period_end_from(
    start: DateTime<Utc>,
    interval: BillingInterval,
) -> Result<DateTime<Utc>, BillingPeriodError> {
    start
        .checked_add_months(Months::new(interval.months()))
        .ok_or(BillingPeriodError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    #[test]
    fn monthly_clamps_to_last_day_of_february() {
        let end = period_end_from(utc(2026, 1, 31), BillingInterval::Monthly).unwrap();
        assert_eq!(end, utc(2026, 2, 28));
    }

    #[test]
    fn monthly_respects_leap_year() {
        let end = period_end_from(utc(2024, 1, 31), BillingInterval::Monthly).unwrap();
        assert_eq!(end, utc(2024, 2, 29));
    }

    #[test]
    fn annual_from_feb_29_clamps_to_feb_28() {
        let end = period_end_from(utc(2024, 2, 29), BillingInterval::Annual).unwrap();
        assert_eq!(end, utc(2025, 2, 28));
    }

    #[test]
    fn annual_is_exactly_twelve_calendar_months() {
        let end = period_end_from(utc(2026, 3, 15), BillingInterval::Annual).unwrap();
        assert_eq!(end, utc(2027, 3, 15));
    }

    #[test]
    fn monthly_is_one_calendar_month() {
        let end = period_end_from(utc(2026, 3, 15), BillingInterval::Monthly).unwrap();
        assert_eq!(end, utc(2026, 4, 15));
    }
}
