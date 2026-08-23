use chrono::{DateTime, Utc};
use domain::enums::{BillingWebhookEventType, PlanTier};
use domain::shared::ids::TenantId;

#[derive(Debug, Clone)]
pub struct ProcessBillingWebhookCommand {
    pub provider_event_id: String,
    pub event_type: BillingWebhookEventType,
    pub tenant_id: TenantId,
    pub plan_tier: Option<PlanTier>,
    pub cancel_at_period_end: bool,
    pub current_period_ends_at: Option<DateTime<Utc>>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessBillingWebhookOutcome {
    /// `true` if the event was processed for the first time.
    /// `false` means the provider event id had already been seen and the
    /// effects were safely ignored.
    pub processed: bool,
}
