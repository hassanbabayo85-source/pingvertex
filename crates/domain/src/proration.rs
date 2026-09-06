use crate::enums::PlanTier;
use crate::pricing::{PricingConfig, PricingInput, calculate_price};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProrationResult {
    pub old_plan: PlanTier,
    pub new_plan: PlanTier,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub unused_old_plan: Decimal,
    pub remaining_new_plan: Decimal,
    pub calculated_amount: Decimal,
    pub pricing_version: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProrationError {
    InvalidPeriod,
    SamePlan,
    Pricing(crate::pricing::PricingValidationError),
}
impl std::fmt::Display for ProrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPeriod => write!(f, "billing period must end after it starts"),
            Self::SamePlan => write!(f, "requested plan is already active"),
            Self::Pricing(e) => write!(f, "pricing error: {e}"),
        }
    }
}
impl std::error::Error for ProrationError {}

pub fn calculate_proration(
    old_plan: PlanTier,
    new_plan: PlanTier,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    effective_at: DateTime<Utc>,
    config: &PricingConfig,
) -> Result<ProrationResult, ProrationError> {
    if old_plan == new_plan {
        return Err(ProrationError::SamePlan);
    }
    if period_end <= period_start {
        return Err(ProrationError::InvalidPeriod);
    }
    let old = calculate_price(input(old_plan), config).map_err(ProrationError::Pricing)?;
    let new = calculate_price(input(new_plan), config).map_err(ProrationError::Pricing)?;
    let effective = effective_at.max(period_start).min(period_end);
    let remaining = (period_end - effective).num_seconds().max(0);
    let total = (period_end - period_start).num_seconds();
    let fraction = Decimal::from(remaining) / Decimal::from(total);
    let unused = (old.monthly_price * fraction).round_dp(2);
    let remaining_new = (new.monthly_price * fraction).round_dp(2);
    Ok(ProrationResult {
        old_plan,
        new_plan,
        period_start,
        period_end,
        unused_old_plan: unused,
        remaining_new_plan: remaining_new,
        calculated_amount: (remaining_new - unused).round_dp(2),
        pricing_version: config.version,
    })
}
fn input(t: PlanTier) -> PricingInput {
    let p = crate::entities::Plan::for_tier(t);
    PricingInput {
        monitors: p.max_monitors as u64,
        interval_seconds: p.min_check_interval_seconds,
        retention_days: p.history_retention_days,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    fn c() -> PricingConfig {
        PricingConfig::default_config()
    }
    fn d(day: u32) -> DateTime<Utc> {
        match day {
            1 => Utc
                .with_ymd_and_hms(2026, 9, 1, 0, 0, 0)
                .single()
                .expect("valid UTC test datetime"),
            10 => Utc
                .with_ymd_and_hms(2026, 9, 10, 0, 0, 0)
                .single()
                .expect("valid UTC test datetime"),
            16 => Utc
                .with_ymd_and_hms(2026, 9, 16, 0, 0, 0)
                .single()
                .expect("valid UTC test datetime"),
            31 => Utc
                .with_ymd_and_hms(2026, 10, 1, 0, 0, 0)
                .single()
                .expect("valid UTC test datetime"),
            _ => panic!("unsupported test day: {day}"),
        }
    }
    #[test]
    fn midpoint_upgrade_is_half_difference() {
        let r = calculate_proration(PlanTier::Starter, PlanTier::Pro, d(1), d(31), d(16), &c())
            .unwrap();
        assert_eq!(
            r.calculated_amount,
            (r.remaining_new_plan - r.unused_old_plan).round_dp(2)
        );
    }
    #[test]
    fn period_end_is_zero() {
        assert_eq!(
            calculate_proration(PlanTier::Pro, PlanTier::Business, d(1), d(31), d(31), &c())
                .unwrap()
                .calculated_amount,
            Decimal::ZERO
        );
    }
    #[test]
    fn period_start_is_full_difference() {
        let r = calculate_proration(
            PlanTier::Starter,
            PlanTier::Business,
            d(1),
            d(31),
            d(1),
            &c(),
        )
        .unwrap();
        assert_eq!(
            r.calculated_amount,
            (r.remaining_new_plan - r.unused_old_plan).round_dp(2)
        );
    }
    #[test]
    fn downgrade_can_be_credit() {
        assert!(
            calculate_proration(
                PlanTier::Business,
                PlanTier::Starter,
                d(1),
                d(31),
                d(10),
                &c()
            )
            .unwrap()
            .calculated_amount
                < Decimal::ZERO
        );
    }
}
