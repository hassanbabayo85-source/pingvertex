use crate::shared::{
    ids::{TenantId, UserId},
    policies::Ownable,
    types::{AuthenticatedUser, TenantContext},
};

/// Resource that belongs to a tenant.
pub trait TenantOwned {
    fn tenant_id(&self) -> TenantId;
}

/// Complete authorization guard.
///
/// Security order:
///
/// 1. Authentication
/// 2. Tenant isolation
/// 3. Ownership
///
/// Fail Closed.
pub struct AuthorizationPolicy;

impl AuthorizationPolicy {
    #[inline]
    pub fn can_access<T>(actor: &AuthenticatedUser, resource: &T) -> bool
    where
        T: Ownable + TenantOwned,
    {
        actor.tenant_context().tenant_id() == resource.tenant_id()
            && actor.user_id() == resource.owner_id()
    }

    #[inline]
    pub fn same_tenant(ctx: &TenantContext, tenant: TenantId) -> bool {
        ctx.tenant_id() == tenant
    }

    #[inline]
    pub fn same_user(actor: &AuthenticatedUser, owner: UserId) -> bool {
        actor.user_id() == owner
    }
}
