use chrono::{DateTime, Utc};

use crate::shared::ids::{TenantId, UserId};
use crate::shared::types::Role;

/// Alakar wani user da wani tenant, tare da matsayin sa (`Role`) a
/// cikin wannan tenant.
///
/// NOTE: `Role` (a `shared::types`) baya aiwatar da `Serialize`/
/// `Deserialize` a yanzu, don haka wannan struct ma baya derive dinsu
/// don kauce wa taɓa `role.rs` (ba a ambace shi cikin fayilolin da za
/// a gyara ba).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantMembership {
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub role: Role,
    pub invited_at: DateTime<Utc>,
    pub joined_at: Option<DateTime<Utc>>,
}
