use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    Open,
    Paid,
    Void,
    Uncollectible,
}
impl InvoiceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Open => "open",
            Self::Paid => "paid",
            Self::Void => "void",
            Self::Uncollectible => "uncollectible",
        }
    }
}
impl std::str::FromStr for InvoiceStatus {
    type Err = String;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        match v {
            "draft" => Ok(Self::Draft),
            "open" => Ok(Self::Open),
            "paid" => Ok(Self::Paid),
            "void" => Ok(Self::Void),
            "uncollectible" => Ok(Self::Uncollectible),
            x => Err(format!("unknown invoice status: {x}")),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceLineType {
    Subscription,
    Addon,
    PaygUsage,
    Credit,
    Refund,
    Tax,
}
impl InvoiceLineType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Subscription => "subscription",
            Self::Addon => "addon",
            Self::PaygUsage => "payg_usage",
            Self::Credit => "credit",
            Self::Refund => "refund",
            Self::Tax => "tax",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceSourceType {
    UsageEvent,
    SubscriptionPeriod,
    WalletTransaction,
}
impl InvoiceSourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UsageEvent => "usage_event",
            Self::SubscriptionPeriod => "subscription_period",
            Self::WalletTransaction => "wallet_transaction",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceLine {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub line_type: InvoiceLineType,
    pub description: String,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub amount: Decimal,
    pub source_type: InvoiceSourceType,
    pub source_id: Uuid,
    pub created_at: DateTime<Utc>,
}
impl InvoiceLine {
    pub fn new(
        id: Uuid,
        invoice_id: Uuid,
        line_type: InvoiceLineType,
        description: String,
        quantity: Decimal,
        unit_price: Decimal,
        source_type: InvoiceSourceType,
        source_id: Uuid,
    ) -> Result<Self, String> {
        if quantity <= Decimal::ZERO {
            return Err("invoice line quantity must be > 0".into());
        }
        Ok(Self {
            id,
            invoice_id,
            line_type,
            description,
            quantity,
            unit_price,
            amount: (quantity * unit_price).round_dp(6),
            source_type,
            source_id,
            created_at: Utc::now(),
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invoice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub invoice_number: String,
    pub currency: String,
    pub subtotal: Decimal,
    pub tax: Decimal,
    pub total: Decimal,
    pub status: InvoiceStatus,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}
