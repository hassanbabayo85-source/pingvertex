//! Task 8B — proration for fixed-price subscription plan changes.
//!
//! Proration compares two monthly prices (from `plan_prices`) over the
//! remaining fraction of the current billing period. It does NOT use the
//! retired PAYG cost model. Downgrades are rejected at the use-case layer:
//! mid-cycle downgrades leave the period unchanged, so any unused
//! remainder is handled at period end, not as a wallet credit.

use crate::enums::PlanTier;
use crate::money::{BillingInterval, Money};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProrationResult {
    pub old_plan: PlanTier,
    pub new_plan: PlanTier,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    /// Unused portion of the OLD plan's monthly price for the remaining
    /// fraction of the period. Money, integer minor units.
    pub unused_old_plan: Money,
    /// Charge for the NEW plan for the same remaining fraction.
    pub remaining_new_plan: Money,
    /// `remaining_new_plan - unused_old_plan`. Positive for upgrades.
    pub calculated_amount: Money,
    /// Version of the new plan's price row that produced this result.
    pub price_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProrationError {
    #[error("billing period must end after it starts")]
    InvalidPeriod,
    #[error("requested plan is already active")]
    SamePlan,
    #[error("currency mismatch: old price is {old:?}, new price is {new:?}")]
    CurrencyMismatch { old: String, new: String },
    #[error("proration arithmetic overflow")]
    Overflow,
}

/// Prorates an upgrade from `old_monthly` to `new_monthly` over the
/// remaining fraction of `[period_start, period_end)`.
///
/// Both prices MUST be monthly, in the same currency. The function does
/// not enforce an upgrade — it returns the signed difference so callers
/// can decide what to do with a downgrade.
pub fn calculate_proration_from_money(
    old_plan: PlanTier,
    new_plan: PlanTier,
    old_monthly: Money,
    new_monthly: Money,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    effective_at: DateTime<Utc>,
    price_version: i64,
) -> Result<ProrationResult, ProrationError> {
    if old_plan == new_plan {
        return Err(ProrationError::SamePlan);
    }
    if period_end <= period_start {
        return Err(ProrationError::InvalidPeriod);
    }
    if old_monthly.currency != new_monthly.currency {
        return Err(ProrationError::CurrencyMismatch {
            old: old_monthly.currency.as_str().to_owned(),
            new: new_monthly.currency.as_str().to_owned(),
        });
    }

    // Only the *remaining* fraction of the period is prorated.
    let effective = effective_at.max(period_start).min(period_end);
    let remaining_secs = (period_end - effective).num_seconds().max(0) as i128;
    let total_secs = (period_end - period_start).num_seconds() as i128;
    if total_secs <= 0 {
        return Err(ProrationError::InvalidPeriod);
    }

    // Integer-safe: (amount_minor * remaining) / total, rounded to nearest.
    let prorate = |amount_minor: i64| -> Result<i64, ProrationError> {
        let num = (amount_minor as i128)
            .checked_mul(remaining_secs)
            .ok_or(ProrationError::Overflow)?;
        // Round half away from zero.
        let (num, den) = if num >= 0 {
            (num + total_secs / 2, total_secs)
        } else {
            (num - total_secs / 2, total_secs)
        };
        let out = num / den;
        i64::try_from(out).map_err(|_| ProrationError::Overflow)
    };

    let unused_old_minor = prorate(old_monthly.amount_minor)?;
    let remaining_new_minor = prorate(new_monthly.amount_minor)?;
    let calculated_minor = remaining_new_minor
        .checked_sub(unused_old_minor)
        .ok_or(ProrationError::Overflow)?;

    Ok(ProrationResult {
        old_plan,
        new_plan,
        period_start,
        period_end,
        unused_old_plan: Money {
            amount_minor: unused_old_minor,
            currency: old_monthly.currency,
        },
        remaining_new_plan: Money {
            amount_minor: remaining_new_minor,
            currency: new_monthly.currency,
        },
        calculated_amount: Money {
            amount_minor: calculated_minor,
            currency: new_monthly.currency,
        },
        price_version,
    })
}

/// Converts a `Money` amount to `Decimal` in major units (divide by 100).
/// Used for persisting the legacy `SubscriptionPlanChange.calculated_amount`
/// NUMERIC column without a schema migration.
pub fn money_to_decimal(money: Money) -> Decimal {
    Decimal::from(money.amount_minor) / Decimal::from(100)
}

/// Returns `BillingInterval::Monthly` — plan changes always prorate the
/// monthly price regardless of the tenant's current interval.
pub fn plan_change_interval() -> BillingInterval {
    BillingInterval::Monthly
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;
    use chrono::TimeZone;

    fn usd(minor: i64) -> Money {
        Money {
            amount_minor: minor,
            currency: Currency::Usd,
        }
    }

    fn d(day: u32) -> DateTime<Utc> {
        let (month, day) = if day == 31 { (10, 1) } else { (9, day) };
        Utc.with_ymd_and_hms(2026, month, day, 0, 0, 0)
            .single()
            .expect("valid UTC test datetime")
    }

    #[test]
    fn midpoint_upgrade_is_half_difference() {
        // Starter $5 = 500c, Pro $19 = 1900c, half period remaining.
        let r = calculate_proration_from_money(
            PlanTier::Starter,
            PlanTier::Pro,
            usd(500),
            usd(1900),
            d(1),
            d(31),
            d(16),
            1,
        )
        .unwrap();
        // Half of (1900 - 500) = 700.
        assert_eq!(r.calculated_amount.amount_minor, 700);
        assert_eq!(r.unused_old_plan.amount_minor, 250);
        assert_eq!(r.remaining_new_plan.amount_minor, 950);
    }

    #[test]
    fn period_end_yields_zero() {
        let r = calculate_proration_from_money(
            PlanTier::Pro,
            PlanTier::Business,
            usd(1900),
            usd(4900),
            d(1),
            d(31),
            d(31),
            1,
        )
        .unwrap();
        assert_eq!(r.calculated_amount.amount_minor, 0);
    }

    #[test]
    fn period_start_yields_full_difference() {
        let r = calculate_proration_from_money(
            PlanTier::Starter,
            PlanTier::Business,
            usd(500),
            usd(4900),
            d(1),
            d(31),
            d(1),
            1,
        )
        .unwrap();
        assert_eq!(r.calculated_amount.amount_minor, 4400);
    }

    #[test]
    fn downgrade_returns_negative_amount() {
        let r = calculate_proration_from_money(
            PlanTier::Business,
            PlanTier::Starter,
            usd(4900),
            usd(500),
            d(1),
            d(31),
            d(10),
            1,
        )
        .unwrap();
        assert!(r.calculated_amount.amount_minor < 0);
    }

    #[test]
    fn same_plan_is_rejected() {
        let err = calculate_proration_from_money(
            PlanTier::Pro,
            PlanTier::Pro,
            usd(1900),
            usd(1900),
            d(1),
            d(31),
            d(16),
            1,
        )
        .unwrap_err();
        assert_eq!(err, ProrationError::SamePlan);
    }

    #[test]
    fn invalid_period_is_rejected() {
        let err = calculate_proration_from_money(
            PlanTier::Starter,
            PlanTier::Pro,
            usd(500),
            usd(1900),
            d(31),
            d(1),
            d(16),
            1,
        )
        .unwrap_err();
        assert_eq!(err, ProrationError::InvalidPeriod);
    }

    #[test]
    fn money_to_decimal_divides_by_100() {
        assert_eq!(money_to_decimal(usd(700)), Decimal::new(7, 0));
        assert_eq!(money_to_decimal(usd(1950)), Decimal::new(195, 1));
    }
}
