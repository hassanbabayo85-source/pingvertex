use chrono::{DateTime, Utc};
use domain::TenantMembership;
use domain::shared::types::Role;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangeMemberRoleRequest {
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct TenantMembershipResponse {
    pub user_id: Uuid,
    pub role: Role,
    pub invited_at: DateTime<Utc>,
    pub joined_at: Option<DateTime<Utc>>,
}

impl From<TenantMembership> for TenantMembershipResponse {
    fn from(m: TenantMembership) -> Self {
        Self {
            user_id: m.user_id.0,
            role: m.role,
            invited_at: m.invited_at,
            joined_at: m.joined_at,
        }
    }
}

impl From<&TenantMembership> for TenantMembershipResponse {
    fn from(m: &TenantMembership) -> Self {
        Self {
            user_id: m.user_id.0,
            role: m.role,
            invited_at: m.invited_at,
            joined_at: m.joined_at,
        }
    }
}
