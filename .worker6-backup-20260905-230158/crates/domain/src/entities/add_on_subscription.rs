use crate::shared::ids::{AddOnSubscriptionId, TenantId};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddOnType {
    SmsCredits,
    EnterpriseSla,
    Rum,
}
impl AddOnType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SmsCredits => "sms",
            Self::EnterpriseSla => "enterprise",
            Self::Rum => "rum",
        }
    }
}
impl std::str::FromStr for AddOnType {
    type Err = String;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        match v {
            "sms" => Ok(Self::SmsCredits),
            "enterprise" => Ok(Self::EnterpriseSla),
            "rum" => Ok(Self::Rum),
            x => Err(format!("unknown add-on type: {x}")),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddOnStatus {
    Pending,
    Active,
    Canceled,
}
impl AddOnStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Canceled => "canceled",
        }
    }
}
impl std::str::FromStr for AddOnStatus {
    type Err = String;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        match v {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "canceled" => Ok(Self::Canceled),
            x => Err(format!("unknown add-on status: {x}")),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddOnSubscription {
    pub id: AddOnSubscriptionId,
    pub tenant_id: TenantId,
    pub addon_type: AddOnType,
    pub quantity: u64,
    pub tier: Option<String>,
    pub price: Decimal,
    pub currency: String,
    pub status: AddOnStatus,
    pub created_at: DateTime<Utc>,
}
impl AddOnSubscription {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: AddOnSubscriptionId,
        tenant_id: TenantId,
        addon_type: AddOnType,
        quantity: u64,
        tier: Option<String>,
        price: Decimal,
        currency: String,
        status: AddOnStatus,
    ) -> Result<Self, String> {
        if quantity == 0 {
            return Err("add-on quantity must be greater than zero".into());
        }
        if price < Decimal::ZERO {
            return Err("add-on price must be >= 0".into());
        }
        if currency.trim().is_empty() {
            return Err("add-on currency must not be empty".into());
        }
        Ok(Self {
            id,
            tenant_id,
            addon_type,
            quantity,
            tier,
            price,
            currency,
            status,
            created_at: Utc::now(),
        })
    }
    pub fn total_price(&self) -> Decimal {
        self.price * Decimal::from(self.quantity)
    }
}
