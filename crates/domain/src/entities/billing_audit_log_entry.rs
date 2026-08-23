use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::BillingWebhookEventType;
use crate::shared::ids::{AuditLogEntryId, SubscriptionId, TenantId};

/// Append-only billing audit log entry.
///
/// Unlike the general `AuditLogEntry`, this record does NOT require an
/// actor `UserId`: billing webhooks are machine events sent by the
/// billing provider, not user actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingAuditLogEntry {
    pub id: AuditLogEntryId,
    pub provider_event_id: String,
    pub event_type: BillingWebhookEventType,
    pub tenant_id: TenantId,
    pub subscription_id: Option<SubscriptionId>,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl BillingAuditLogEntry {
    pub fn new(
        id: AuditLogEntryId,
        provider_event_id: String,
        event_type: BillingWebhookEventType,
        tenant_id: TenantId,
        subscription_id: Option<SubscriptionId>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id,
            provider_event_id,
            event_type,
            tenant_id,
            subscription_id,
            payload,
            created_at: Utc::now(),
        }
    }
}
