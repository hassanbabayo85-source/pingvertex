use crate::shared::ids::{TenantId, UserId};

/// Request-scoped tenant boundary.
///
/// Every tenant-aware operation should carry this context
/// so repositories and application guards can enforce isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TenantContext {
    tenant_id: TenantId,
    actor_user_id: Option<UserId>,
}

impl TenantContext {
    pub fn new(tenant_id: TenantId) -> Self {
        Self {
            tenant_id,
            actor_user_id: None,
        }
    }

    pub fn with_actor(tenant_id: TenantId, actor_user_id: UserId) -> Self {
        Self {
            tenant_id,
            actor_user_id: Some(actor_user_id),
        }
    }

    pub fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }

    pub fn actor_user_id(&self) -> Option<UserId> {
        self.actor_user_id
    }

    pub fn is_acted_by_user(&self) -> bool {
        self.actor_user_id.is_some()
    }
}
