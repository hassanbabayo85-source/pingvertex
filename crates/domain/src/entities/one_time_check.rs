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
