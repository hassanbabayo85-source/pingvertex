//! Integer minor-unit money, currency, and billing-interval types.
//!
//! This module is self-contained and does not depend on `DomainError` so
//! that adding it cannot alter the existing `DomainError` surface (which
//! is not covered by TASK 1's change set).
//!
//! Money is NEVER represented with floating-point. Every amount is a
//! signed integer count of the currency's smallest unit:
//!   * NGN -> kobo  (1 NGN = 100 kobo)
//!   * USD -> cents (1 USD = 100 cents)

use serde::{Deserialize, Serialize};

// ─── Currency ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Ngn,
    Usd,
}

impl Currency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ngn => "NGN",
            Self::Usd => "USD",
        }
    }

    /// ISO-4217 minor-unit exponent. Both supported currencies use 2
    /// decimal places in their minor unit.
    pub fn minor_unit_exponent(&self) -> u32 {
        2
    }

    /// Human symbol used by `Money::format_minor`.
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Ngn => "₦",
            Self::Usd => "$",
        }
    }
}

impl std::str::FromStr for Currency {
    type Err = MoneyError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_uppercase().as_str() {
            "NGN" => Ok(Self::Ngn),
            "USD" => Ok(Self::Usd),
            other => Err(MoneyError::UnknownCurrency(other.to_owned())),
        }
    }
}

// ─── BillingInterval ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BillingInterval {
    Monthly,
    Annual,
}

impl BillingInterval {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Monthly => "monthly",
            Self::Annual => "annual",
        }
    }

    /// Number of whole months this interval covers.
    pub fn months(&self) -> u32 {
        match self {
            Self::Monthly => 1,
            Self::Annual => 12,
        }
    }
}

impl std::str::FromStr for BillingInterval {
    type Err = MoneyError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "monthly" => Ok(Self::Monthly),
            "annual" | "yearly" => Ok(Self::Annual),
            other => Err(MoneyError::UnknownBillingInterval(other.to_owned())),
        }
    }
}

// ─── Money ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    pub amount_minor: i64,
    pub currency: Currency,
}

impl Money {
    /// Constructs a money value. Rejects negative amounts: plan prices,
    /// payments, and invoices are always non-negative on this product.
    pub fn new(amount_minor: i64, currency: Currency) -> Result<Self, MoneyError> {
        if amount_minor < 0 {
            return Err(MoneyError::NegativeAmount(amount_minor));
        }
        Ok(Self {
            amount_minor,
            currency,
        })
    }

    pub fn zero(currency: Currency) -> Self {
        Self {
            amount_minor: 0,
            currency,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.amount_minor == 0
    }

    pub fn checked_add(self, other: Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch {
                left: self.currency,
                right: other.currency,
            });
        }
        let sum = self
            .amount_minor
            .checked_add(other.amount_minor)
            .ok_or(MoneyError::Overflow)?;
        Money::new(sum, self.currency)
    }

    pub fn checked_mul(self, quantity: u32) -> Result<Money, MoneyError> {
        let product = self
            .amount_minor
            .checked_mul(i64::from(quantity))
            .ok_or(MoneyError::Overflow)?;
        Money::new(product, self.currency)
    }

    /// Formats the value for display, e.g. `"₦8000.00"`, `"$19.00"`.
    /// Display-only; never used for arithmetic.
    pub fn format_minor(&self) -> String {
        let divisor = 10_i64.pow(self.currency.minor_unit_exponent());
        let whole = self.amount_minor / divisor;
        let frac = (self.amount_minor % divisor).abs();
        format!("{}{}.{:02}", self.currency.symbol(), whole, frac)
    }
}

// ─── Error ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoneyError {
    #[error("money amount cannot be negative: {0}")]
    NegativeAmount(i64),
    #[error("currency mismatch: {left:?} vs {right:?}")]
    CurrencyMismatch { left: Currency, right: Currency },
    #[error("money arithmetic overflow")]
    Overflow,
    #[error("unknown currency: {0}")]
    UnknownCurrency(String),
    #[error("unknown billing interval: {0}")]
    UnknownBillingInterval(String),
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn currency_round_trips_through_str() {
        for c in [Currency::Ngn, Currency::Usd] {
            assert_eq!(Currency::from_str(c.as_str()).unwrap(), c);
        }
    }

    #[test]
    fn currency_rejects_unknown() {
        assert!(Currency::from_str("EUR").is_err());
    }

    #[test]
    fn billing_interval_round_trips_through_str() {
        for i in [BillingInterval::Monthly, BillingInterval::Annual] {
            assert_eq!(BillingInterval::from_str(i.as_str()).unwrap(), i);
        }
    }

    #[test]
    fn money_rejects_negative_amounts() {
        assert_eq!(
            Money::new(-1, Currency::Usd),
            Err(MoneyError::NegativeAmount(-1))
        );
    }

    #[test]
    fn money_checked_add_rejects_cross_currency() {
        let usd = Money::new(100, Currency::Usd).unwrap();
        let ngn = Money::new(100, Currency::Ngn).unwrap();
        assert!(matches!(
            usd.checked_add(ngn),
            Err(MoneyError::CurrencyMismatch { .. })
        ));
    }

    #[test]
    fn money_formats_usd_cents_and_ngn_kobo() {
        assert_eq!(Money::new(500, Currency::Usd).unwrap().format_minor(), "$5.00");
        assert_eq!(
            Money::new(800_000, Currency::Ngn).unwrap().format_minor(),
            "₦8000.00"
        );
        assert_eq!(Money::new(0, Currency::Usd).unwrap().format_minor(), "$0.00");
    }

    #[test]
    fn money_checked_mul() {
        let m = Money::new(1900, Currency::Usd).unwrap();
        assert_eq!(m.checked_mul(3).unwrap().amount_minor, 5700);
    }
}
