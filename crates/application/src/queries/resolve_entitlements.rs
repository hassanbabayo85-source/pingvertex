use domain::shared::ids::TenantId;

/// Query for resolving a tenant's current plan entitlements.
#[derive(Debug, Clone, Copy)]
pub struct ResolveEntitlements {
    pub tenant_id: TenantId,
}
