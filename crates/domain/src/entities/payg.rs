use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Immutable, server-controlled PAYG price for one billable unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PAYGPricing {
    pub id: Uuid,
    pub version: i64,
    pub operation: String,
    pub unit_name: String,
    pub unit_price: i64,
    pub currency: String,
    pub effective_from: DateTime<Utc>,
    pub effective_until: Option<DateTime<Utc>>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageEvent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub operation: String,
    pub units_requested: i64,
    pub units_billed: i64,
    pub unit_price: i64,
    pub charge_units: i64,
    pub currency: String,
    pub pricing_version: i64,
    pub request_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub amount_units: i64,
    pub currency: String,
    pub status: String,
    pub provider: String,
    pub provider_payment_id: Option<String>,
    pub idempotency_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
