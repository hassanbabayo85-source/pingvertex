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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletBalanceSummary {
    pub balance_units: i64,
    pub reserved_units: i64,
    pub available_units: i64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletSettings {
    pub tenant_id: Uuid,
    pub low_balance_threshold_units: i64,
    pub critical_balance_threshold_units: i64,
    pub auto_top_up_enabled: bool,
    pub auto_top_up_threshold_units: i64,
    pub auto_top_up_amount_units: i64,
    pub payment_method: Option<String>,
    pub daily_top_up_limit_units: i64,
    pub monthly_top_up_limit_units: i64,
    pub retry_limit: i32,
}

impl Default for WalletSettings {
    fn default() -> Self {
        Self {
            tenant_id: Uuid::nil(),
            low_balance_threshold_units: 5_000_000,
            critical_balance_threshold_units: 1_000_000,
            auto_top_up_enabled: false,
            auto_top_up_threshold_units: 5_000_000,
            auto_top_up_amount_units: 20_000_000,
            payment_method: None,
            daily_top_up_limit_units: 100_000_000,
            monthly_top_up_limit_units: 500_000_000,
            retry_limit: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoTopUpCandidate {
    pub tenant_id: Uuid,
    pub balance_units: i64,
    pub settings: WalletSettings,
    pub daily_spend_units: i64,
    pub monthly_spend_units: i64,
    pub daily_attempts: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletNotification {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub event_type: String,
    pub dedupe_key: String,
    pub message: String,
    pub read_at: Option<DateTime<Utc>>,
    pub email_sent_at: Option<DateTime<Utc>>,
    pub email_processing_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletTopUpSession {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider: String,
    pub provider_checkout_id: String,
    pub provider_payment_id: Option<String>,
    pub amount_units: i64,
    pub refunded_amount_units: i64,
    pub currency: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WalletTopUpSession {
    pub const PENDING: &'static str = "PENDING";
    pub const COMPLETED: &'static str = "COMPLETED";
    pub const FAILED: &'static str = "FAILED";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoTopUpAttempt {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub amount_units: i64,
    pub status: String,
    pub provider_reference: Option<String>,
    pub error_message: Option<String>,
    pub attempted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletRefundRequest {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider_payment_id: String,
    pub amount_units: i64,
    pub idempotency_key: String,
    pub provider_refund_id: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
