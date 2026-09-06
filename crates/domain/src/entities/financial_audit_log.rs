use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durable audit record for financial/security-sensitive billing actions.
/// `actor` is deliberately a string so both authenticated users and trusted
/// machine actors (for example `billing:webhook`) can be represented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinancialAuditLogEntry {
    pub id: Uuid,
    pub actor: String,
    pub tenant_id: Option<Uuid>,
    pub action: String,
    pub resource: String,
    pub resource_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl FinancialAuditLogEntry {
    pub fn new(
        actor: impl Into<String>,
        tenant_id: Option<Uuid>,
        action: impl Into<String>,
        resource: impl Into<String>,
        resource_id: Option<String>,
        metadata: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            actor: actor.into(),
            tenant_id,
            action: action.into(),
            resource: resource.into(),
            resource_id,
            created_at: Utc::now(),
            metadata,
        }
    }
}
