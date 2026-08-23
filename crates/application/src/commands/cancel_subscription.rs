use chrono::{DateTime, Utc};
use domain::shared::ids::TenantId;

/// Command to cancel a tenant's subscription, either immediately or at
/// the end of the current billing period.
#[derive(Debug, Clone, Copy)]
pub struct CancelSubscription {
    pub tenant_id: TenantId,
    pub at_period_end: bool,
    /// Required (and only meaningful) when `at_period_end` is `true`.
    pub current_period_ends_at: Option<DateTime<Utc>>,
}
