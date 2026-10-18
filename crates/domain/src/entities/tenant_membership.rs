use chrono::{DateTime, Utc};

use crate::shared::ids::{TenantId, UserId};
use crate::shared::types::Role;

/// The relationship between a user and a tenant, together with that user's
/// role (`Role`) within the tenant.
///
/// NOTE: `Role` (in `shared::types`) does not currently implement
/// `Serialize`/`Deserialize`, so this struct does not derive them either,
/// which avoids having to touch `role.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantMembership {
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub role: Role,
    pub invited_at: DateTime<Utc>,
    pub joined_at: Option<DateTime<Utc>>,
}
