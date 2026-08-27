use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{AuditLogEntryId, UserId};

/// Rikodin audit log na musamman ga canje-canjen pricing config.
/// Yana biye da schema na `pricing_audit_logs` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PricingAuditLogEntry {
    pub id: AuditLogEntryId,
    pub version: u32,
    pub field_name: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub changed_by: Option<UserId>,
    pub changed_at: DateTime<Utc>,
    pub reason: Option<String>,
}

impl PricingAuditLogEntry {
    pub fn new(
        id: AuditLogEntryId,
        version: u32,
        field_name: String,
        old_value: Option<String>,
        new_value: Option<String>,
        changed_by: Option<UserId>,
        reason: Option<String>,
    ) -> Self {
        Self {
            id,
            version,
            field_name,
            old_value,
            new_value,
            changed_by,
            changed_at: Utc::now(),
            reason,
        }
    }
}
