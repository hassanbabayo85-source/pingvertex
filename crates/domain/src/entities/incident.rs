use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{IncidentId, MonitorId};

/// Represents a downtime incident.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: IncidentId,
    pub monitor_id: MonitorId,

    pub started_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
}
