use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::AuditAction;
use crate::shared::ids::{AuditLogEntryId, TenantId, UserId};

/// An audit log record, append-only by design.
///
/// This struct has NO `mut` setters and no `update`/`delete` method
/// ANYWHERE. The application persistence layer likewise has no mutation port,
/// and the PostgreSQL migration blocks UPDATE/DELETE at the database level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: AuditLogEntryId,
    pub actor_user_id: UserId,
    pub tenant_id: Option<TenantId>,
    pub action: AuditAction,
    pub target_id: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
