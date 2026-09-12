use chrono::{DateTime, Utc};

use crate::{
    shared::ids::{MonitorId, SubscriberId, TenantId},
    shared::policies::TenantOwned,
    value_objects::Email,
};

#[derive(Debug, Clone)]
pub struct CreateSubscriber {
    pub id: SubscriberId,
    pub monitor_id: MonitorId,
    pub tenant_id: TenantId,
    pub email: Email,
}

#[derive(Debug, Clone)]
pub struct Subscriber {
    pub id: SubscriberId,
    pub monitor_id: MonitorId,
    pub tenant_id: TenantId,
    pub email: Email,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
}

impl Subscriber {
    pub fn create(input: CreateSubscriber) -> Self {
        Self {
            id: input.id,
            monitor_id: input.monitor_id,
            tenant_id: input.tenant_id,
            email: input.email,
            verified: false,
            created_at: Utc::now(),
        }
    }

    pub fn verify(&mut self) {
        self.verified = true;
    }
}

// SEC-P0-3: subscribers is a monitor-owned child table, so it carries its
// own tenant_id (backed by the composite FK added in migration 0066) —
// the same pattern used by MaintenanceWindow, EscalationPolicy, etc. since
// the 2026-09 cross-tenant integrity review (see migration 0065).
impl TenantOwned for Subscriber {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}
