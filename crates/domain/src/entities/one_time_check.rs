use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneTimeCheckRequest {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub monitor_id: Uuid,
    pub created_by: Uuid,
    pub request_id: String,
    pub status: String,
    pub execution_id: Uuid,
    pub lease_owner: Option<Uuid>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl OneTimeCheckRequest {
    pub const QUEUED: &'static str = "QUEUED";
    pub const RUNNING: &'static str = "RUNNING";
    pub const SUCCEEDED: &'static str = "SUCCEEDED";
    pub const FAILED: &'static str = "FAILED";
}

/// One independent, billable regional execution of a one-time check.
///
/// Each row has its own durable `execution_id` used as the wallet
/// idempotency component, so a scheduler retry of one region can never
/// double-charge while other regions are still running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneTimeCheckExecution {
    pub id: Uuid,
    pub request_id: Uuid,
    pub tenant_id: Uuid,
    pub monitor_id: Uuid,
    pub region: String,
    pub execution_id: Uuid,
    pub status: String,
    pub lease_owner: Option<Uuid>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub attempted: bool,
    pub billable: bool,
    pub charge_units: i64,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl OneTimeCheckExecution {
    pub const QUEUED: &'static str = "QUEUED";
    pub const RUNNING: &'static str = "RUNNING";
    pub const SUCCEEDED: &'static str = "SUCCEEDED";
    pub const FAILED: &'static str = "FAILED";
}
