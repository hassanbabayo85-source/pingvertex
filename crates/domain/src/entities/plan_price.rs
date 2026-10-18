//! Versioned fixed-price record for a plan.
//!
//! Prices are immutable once written. A price change inserts a NEW row
//! with a higher `version`. Historical purchases keep the price_id they
//! paid for.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::money::{BillingInterval, Currency, Money};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanPrice {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub currency: Currency,
    pub billing_interval: BillingInterval,
    /// Money in minor units for this currency and interval.
    pub amount: Money,
    /// Monotonic version for `(plan_id, currency, billing_interval)`.
    pub version: i64,
    pub effective_from: DateTime<Utc>,
    pub effective_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl PlanPrice {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        plan_id: Uuid,
        currency: Currency,
        billing_interval: BillingInterval,
        amount: Money,
        version: i64,
        effective_from: DateTime<Utc>,
    ) -> Result<Self, PlanPriceError> {
        if amount.currency != currency {
            return Err(PlanPriceError::CurrencyMismatch {
                declared: currency,
                amount: amount.currency,
            });
        }
        if version < 1 {
            return Err(PlanPriceError::NonPositiveVersion(version));
        }
        Ok(Self {
            id,
            plan_id,
            currency,
            billing_interval,
            amount,
            version,
            effective_from,
            effective_until: None,
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanPriceError {
    #[error("plan price currency {declared:?} does not match amount currency {amount:?}")]
    CurrencyMismatch {
        declared: Currency,
        amount: Currency,
    },
    #[error("plan price version must be >= 1, got {0}")]
    NonPositiveVersion(i64),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usd(amount_minor: i64) -> Money {
        Money::new(amount_minor, Currency::Usd).unwrap()
    }

    #[test]
    fn new_rejects_currency_mismatch() {
        let result = PlanPrice::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Currency::Ngn,
            BillingInterval::Monthly,
            usd(500),
            1,
            Utc::now(),
        );
        assert!(matches!(
            result,
            Err(PlanPriceError::CurrencyMismatch { .. })
        ));
    }

    #[test]
    fn new_rejects_zero_version() {
        let result = PlanPrice::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Currency::Usd,
            BillingInterval::Monthly,
            usd(500),
            0,
            Utc::now(),
        );
        assert_eq!(result, Err(PlanPriceError::NonPositiveVersion(0)));
    }

    #[test]
    fn new_accepts_valid_input() {
        let now = Utc::now();
        let plan = PlanPrice::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Currency::Usd,
            BillingInterval::Monthly,
            usd(500),
            1,
            now,
        )
        .unwrap();
        assert_eq!(plan.amount.amount_minor, 500);
        assert_eq!(plan.version, 1);
        assert!(plan.effective_until.is_none());
    }
}
