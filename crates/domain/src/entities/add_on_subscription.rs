use crate::shared::ids::{AddOnSubscriptionId, TenantId};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddOnType {
    SmsCredits,
    EnterpriseSla,
    ExtendedRetention,
}
impl AddOnType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SmsCredits => "sms",
            Self::EnterpriseSla => "enterprise",
            Self::ExtendedRetention => "retention",
        }
    }
}
impl std::str::FromStr for AddOnType {
    type Err = String;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        match v {
            "sms" => Ok(Self::SmsCredits),
            "enterprise" => Ok(Self::EnterpriseSla),
            "retention" => Ok(Self::ExtendedRetention),
            x => Err(format!("unknown add-on type: {x}")),
        }
    }
}

/// Extended-retention tier for `AddOnType::ExtendedRetention`.
///
/// Retention is a *tier* (a service level), not a consumable quantity:
/// `AddOnSubscription.quantity` is always 1 for this add-on, and
/// `AddOnSubscription.tier` carries the tier as `"90" | "180" | "365"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionTier {
    D90,
    D180,
    D365,
}

impl RetentionTier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::D90 => "90",
            Self::D180 => "180",
            Self::D365 => "365",
        }
    }

    pub const fn days(self) -> u32 {
        match self {
            Self::D90 => 90,
            Self::D180 => 180,
            Self::D365 => 365,
        }
    }
}

impl std::str::FromStr for RetentionTier {
    type Err = String;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        match v {
            "90" => Ok(Self::D90),
            "180" => Ok(Self::D180),
            "365" => Ok(Self::D365),
            x => Err(format!(
                "unknown retention tier: {x}; expected 90, 180, or 365"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddOnStatus {
    Pending,
    Active,
    Canceled,
    Failed,
}
impl AddOnStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Canceled => "canceled",
            Self::Failed => "failed",
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
            "failed" => Ok(Self::Failed),
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
    pub provider_checkout_id: Option<String>,
    pub paid_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
        let now = Utc::now();
        Ok(Self {
            id,
            tenant_id,
            addon_type,
            quantity,
            tier,
            price,
            currency,
            status,
            provider_checkout_id: None,
            paid_at: None,
            canceled_at: None,
            created_at: now,
            updated_at: now,
        })
    }
    pub fn total_price(&self) -> Decimal {
        self.price * Decimal::from(self.quantity)
    }
    pub fn attach_checkout(&mut self, id: String) {
        self.provider_checkout_id = Some(id);
        self.updated_at = Utc::now()
    }
    pub fn activate_after_verified_payment(&mut self) -> Result<(), String> {
        if self.status != AddOnStatus::Pending {
            return Err(format!("cannot activate add-on from {:?}", self.status));
        }
        self.status = AddOnStatus::Active;
        self.paid_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }
    pub fn mark_failed(&mut self) -> Result<(), String> {
        if self.status != AddOnStatus::Pending {
            return Err(format!("cannot fail add-on from {:?}", self.status));
        }
        self.status = AddOnStatus::Failed;
        self.updated_at = Utc::now();
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<(), String> {
        if self.status != AddOnStatus::Active {
            return Err(format!("cannot cancel add-on from {:?}", self.status));
        }
        self.status = AddOnStatus::Canceled;
        self.canceled_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }
}
