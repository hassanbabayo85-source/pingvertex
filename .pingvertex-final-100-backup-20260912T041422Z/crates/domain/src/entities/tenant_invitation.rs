use chrono::{DateTime, Utc};

use crate::shared::ids::{InvitationId, TenantId};
use crate::shared::types::Role;

/// A single-use, expiring invitation for a non-member email address to
/// join a tenant with a specific role.
///
/// The raw token is never persisted; only its Argon2id hash is stored,
/// alongside a plaintext lookup prefix (same pattern as email
/// verification, password reset, and API keys).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantInvitation {
    pub id: InvitationId,
    pub tenant_id: TenantId,
    pub email: String,
    pub role: Role,
    pub token_prefix: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl TenantInvitation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: InvitationId,
        tenant_id: TenantId,
        email: String,
        role: Role,
        token_prefix: String,
        token_hash: String,
        expires_at: DateTime<Utc>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            tenant_id,
            email,
            role,
            token_prefix,
            token_hash,
            expires_at,
            revoked_at: None,
            accepted_at: None,
            created_at,
        }
    }

    /// Pending = not revoked, not accepted, and not expired.
    pub fn is_pending(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.accepted_at.is_none() && self.expires_at > now
    }

    pub fn mark_revoked(&mut self, when: DateTime<Utc>) {
        self.revoked_at = Some(when);
    }

    pub fn mark_accepted(&mut self, when: DateTime<Utc>) {
        self.accepted_at = Some(when);
    }
}
