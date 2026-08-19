use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::AuditAction;
use crate::shared::ids::{AuditLogEntryId, TenantId, UserId};

/// Rikodin audit log, append-only ta hanyar design.
///
/// Wannan struct BABU `mut` setters ko `update`/`delete` method
/// KO'INA — babu DB-level REVOKE don haka domain/application layer
/// SHI NE layin farko na kariya akan gyara/goge rikodin audit bayan
/// an kirkiresu.
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
