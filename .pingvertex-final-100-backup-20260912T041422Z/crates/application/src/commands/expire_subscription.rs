use domain::shared::ids::TenantId;

/// Command to expire a tenant's subscription (grace period or trial ran
/// out with no successful payment).
#[derive(Debug, Clone, Copy)]
pub struct ExpireSubscription {
    pub tenant_id: TenantId,
}
