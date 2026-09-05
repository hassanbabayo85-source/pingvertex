use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Invoice domain shape only. Invoice generation/settlement belongs to Worker 6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invoice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub number: String,
    pub status: String,
    pub currency: String,
    pub subtotal_units: i64,
    pub total_units: i64,
    pub issued_at: Option<DateTime<Utc>>,
    pub due_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
