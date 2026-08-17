use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{IncidentId, MonitorId, UserId};

/// Represents a downtime incident.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: IncidentId,
    pub monitor_id: MonitorId,

    pub started_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Acknowledgement fields
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<UserId>,

    // Maintenance window flag
    pub occurred_during_maintenance: bool,

    // Escalation progress
    pub escalation_step_index: u32,
    pub last_escalation_at: Option<DateTime<Utc>>,
}

impl Incident {
    pub fn new(id: IncidentId, monitor_id: MonitorId) -> Self {
        let now = Utc::now();

        Self {
            id,
            monitor_id,
            started_at: now,
            resolved_at: None,
            created_at: now,
            updated_at: now,
            acknowledged_at: None,
            acknowledged_by: None,
            occurred_during_maintenance: false,
            escalation_step_index: 0,
            last_escalation_at: None,
        }
    }

    pub fn resolve(&mut self) {
        let now = Utc::now();

        self.resolved_at = Some(now);
        self.updated_at = now;
    }

    pub fn is_open(&self) -> bool {
        self.resolved_at.is_none()
    }

    /// Yana amincewa da incident. Yana sanya `acknowledged_at` da `acknowledged_by`.
    pub fn acknowledge(&mut self, user_id: UserId, now: DateTime<Utc>) {
        self.acknowledged_at = Some(now);
        self.acknowledged_by = Some(user_id);
        self.updated_at = now;
    }

    /// Yana sanya alamar cewa incident ɗin ya faru a lokacin maintenance window.
    pub fn mark_occurred_during_maintenance(&mut self) {
        self.occurred_during_maintenance = true;
    }
}
