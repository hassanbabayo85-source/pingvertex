use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::BillingWebhookEventType;
use crate::enums::PlanTier;
use crate::shared::ids::TenantId;

/// A parsed, provider-agnostic billing webhook event.
///
/// This is the domain representation after the API handler has already
/// verified the provider signature and decoded the provider payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingWebhookEvent {
    pub provider_event_id: String,
    pub event_type: BillingWebhookEventType,
    pub tenant_id: TenantId,
    pub plan_tier: Option<PlanTier>,
    pub cancel_at_period_end: bool,
    pub current_period_ends_at: Option<DateTime<Utc>>,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl BillingWebhookEvent {
    pub fn new(
        provider_event_id: String,
        event_type: BillingWebhookEventType,
        tenant_id: TenantId,
        plan_tier: Option<PlanTier>,
        cancel_at_period_end: bool,
        current_period_ends_at: Option<DateTime<Utc>>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            provider_event_id,
            event_type,
            tenant_id,
            plan_tier,
            cancel_at_period_end,
            current_period_ends_at,
            payload,
            created_at: Utc::now(),
        }
    }
}
