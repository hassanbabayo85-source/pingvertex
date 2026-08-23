use domain::shared::ids::TenantId;

/// Command to finalize a previously-scheduled end-of-period
/// cancellation once the current billing period has actually ended.
#[derive(Debug, Clone, Copy)]
pub struct FinalizeScheduledCancellation {
    pub tenant_id: TenantId,
}
