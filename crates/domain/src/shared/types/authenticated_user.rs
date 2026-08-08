use crate::shared::ids::{SessionId, UserId};
use crate::shared::types::{Permission, Role, TenantContext};

/// Request-scoped authenticated actor.
///
/// This is the security boundary object used by application
/// services and authorization guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuthenticatedUser {
    user_id: UserId,
    session_id: Option<SessionId>,
    tenant_context: TenantContext,
    role: Role,
}

impl AuthenticatedUser {
    pub fn new(user_id: UserId, role: Role, tenant_context: TenantContext) -> Self {
        Self {
            user_id,
            session_id: None,
            tenant_context,
            role,
        }
    }

    pub fn with_session(
        user_id: UserId,
        session_id: SessionId,
        role: Role,
        tenant_context: TenantContext,
    ) -> Self {
        Self {
            user_id,
            session_id: Some(session_id),
            tenant_context,
            role,
        }
    }

    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    pub fn session_id(&self) -> Option<SessionId> {
        self.session_id
    }

    pub fn tenant_context(&self) -> TenantContext {
        self.tenant_context
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn has_session(&self) -> bool {
        self.session_id.is_some()
    }

    pub fn can(&self, permission: Permission) -> bool {
        self.role.has_permission(permission)
    }
}
