use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalletTransactionType {
    TopUp,
    Debit,
    Refund,
    Adjustment,
    PromotionalCredit,
    Expiration,
}
impl WalletTransactionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopUp => "TOP_UP",
            Self::Debit => "DEBIT",
            Self::Refund => "REFUND",
            Self::Adjustment => "ADJUSTMENT",
            Self::PromotionalCredit => "PROMOTIONAL_CREDIT",
            Self::Expiration => "EXPIRATION",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wallet {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub currency: String,
    pub balance_units: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Wallet {
    pub fn new(id: Uuid, tenant_id: Uuid, currency: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id,
            tenant_id,
            currency: currency.into(),
            balance_units: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletTransaction {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub wallet_id: Uuid,
    pub transaction_type: WalletTransactionType,
    pub amount_units: i64,
    pub balance_after_units: i64,
    pub idempotency_key: String,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditReservation {
    pub id: Uuid,
    pub wallet_id: Uuid,
    pub amount_units: i64,
    pub status: String,
    pub operation: String,
    pub request_id: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl CreditReservation {
    pub const PENDING: &'static str = "PENDING";
    pub const CAPTURED: &'static str = "CAPTURED";
    pub const RELEASED: &'static str = "RELEASED";
    pub const EXPIRED: &'static str = "EXPIRED";
}
