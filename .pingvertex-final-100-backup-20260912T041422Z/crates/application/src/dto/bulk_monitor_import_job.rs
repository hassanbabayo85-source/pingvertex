use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BulkMonitorImportJobStatus {
    Pending,
    Validating,
    Ready,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl BulkMonitorImportJobStatus {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Validating => "VALIDATING",
            Self::Ready => "READY",
            Self::Running => "RUNNING",
            Self::Paused => "PAUSED",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkMonitorImportJob {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub actor_user_id: Uuid,
    pub status: BulkMonitorImportJobStatus,
    pub total_rows: i32,
    pub valid_rows: i32,
    pub invalid_rows: i32,
    pub processed_rows: i32,
    pub created_rows: i32,
    pub failed_rows: i32,
    pub skipped_rows: i32,
    pub forecast_days: i32,
    pub estimated_checks: i64,
    pub estimated_charge_units: i64,
    pub currency: String,
    pub batch_size: i32,
    pub pricing_snapshot: Value,
    pub error_report: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewBulkMonitorImportRow {
    pub row_number: i32,
    pub payload: Value,
    pub validation_status: String,
    pub validation_errors: Option<Value>,
    pub planned_monitor_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct CreateBulkMonitorImportJob {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub actor_user_id: Uuid,
    pub idempotency_key: String,
    pub source_sha256: String,
    pub total_rows: i32,
    pub valid_rows: i32,
    pub invalid_rows: i32,
    pub forecast_days: i32,
    pub estimated_checks: i64,
    pub estimated_charge_units: i64,
    pub currency: String,
    pub batch_size: i32,
    pub pricing_snapshot: Value,
    pub rows: Vec<NewBulkMonitorImportRow>,
}

#[derive(Debug, Clone)]
pub enum CreateBulkMonitorImportJobOutcome {
    Created(BulkMonitorImportJob),
    Existing(BulkMonitorImportJob),
    IdempotencyConflict,
}

#[derive(Debug, Clone)]
pub struct ClaimedBulkMonitorImportRow {
    pub id: i64,
    pub job_id: Uuid,
    pub row_number: i32,
    pub payload: Value,
    pub planned_monitor_id: Uuid,
    pub attempt_count: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulkMonitorImportConfirmResponse {
    pub job: BulkMonitorImportJob,
    pub replayed: bool,
}
