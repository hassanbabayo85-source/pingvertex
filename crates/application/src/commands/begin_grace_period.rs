use chrono::{DateTime, Utc};
use domain::shared::ids::TenantId;

/// Command to move a tenant's subscription into its post-payment-failure
/// grace period.
#[derive(Debug, Clone, Copy)]
pub struct BeginGracePeriod {
    pub tenant_id: TenantId,
    pub grace_period_ends_at: DateTime<Utc>,
}
