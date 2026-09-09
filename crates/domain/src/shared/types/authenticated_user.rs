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
    platform_admin: bool,
}

impl AuthenticatedUser {
    pub fn new(user_id: UserId, role: Role, tenant_context: TenantContext) -> Self {
        Self {
            user_id,
            session_id: None,
            tenant_context,
            role,
            platform_admin: false,
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
            platform_admin: false,
        }
    }

    /// Marks this request actor as a platform administrator after the authentication
    /// boundary has verified the current server-side global account role.
    pub fn with_platform_admin(mut self, platform_admin: bool) -> Self {
        self.platform_admin = platform_admin;
        self
    }

    pub fn is_platform_admin(&self) -> bool {
        self.platform_admin
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

    /// Enforces that this actor holds `permission`, returning
    /// [`PermissionDenied`] if not.
    ///
    /// Use-cases that perform a write or otherwise privileged action MUST
    /// call this (or check `can(...)` directly) before doing any work.
    /// `jwt_auth` only proves *who* the caller is (authentication); it does
    /// not know which permission a given route requires, so authorization
    /// has to happen here, in the use case / application layer.
    pub fn require(&self, permission: Permission) -> Result<(), PermissionDenied> {
        if self.can(permission) {
            Ok(())
        } else {
            Err(PermissionDenied { permission })
        }
    }
}

/// Returned by [`AuthenticatedUser::require`] when the actor's role does not
/// grant the requested [`Permission`]. Callers should map this to an HTTP
/// 403, never a 500 — see `ApiError::forbidden`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionDenied {
    pub permission: Permission,
}

impl std::fmt::Display for PermissionDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "forbidden: your role does not have the {:?} permission",
            self.permission
        )
    }
}

impl std::error::Error for PermissionDenied {}
