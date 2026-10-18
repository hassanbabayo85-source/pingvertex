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

    /// Timestamp of the most recent failure observation while this
    /// incident has been open. Set to `started_at` when the incident is
    /// first created, and advanced by `touch_last_seen` every time a
    /// subsequent failing check is evaluated against an already-open
    /// incident. `None` only for incidents persisted before this field
    /// existed (pre-migration rows read back as `NULL`).
    pub last_seen_at: Option<DateTime<Utc>>,

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
            last_seen_at: Some(now),
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

    /// Acknowledges the incident. Sets `acknowledged_at` and `acknowledged_by`.
    pub fn acknowledge(&mut self, user_id: UserId, now: DateTime<Utc>) {
        self.acknowledged_at = Some(now);
        self.acknowledged_by = Some(user_id);
        self.updated_at = now;
    }

    /// Marks the incident as having occurred during a maintenance window.
    pub fn mark_occurred_during_maintenance(&mut self) {
        self.occurred_during_maintenance = true;
    }

    /// Advances `last_seen_at` to `now`. Called every time a failing
    /// check is observed while this incident is still open, so
    /// `last_seen_at` always reflects the most recent evidence the
    /// underlying problem is ongoing (distinct from `started_at`, which
    /// never changes after creation).
    pub fn touch_last_seen(&mut self, now: DateTime<Utc>) {
        self.last_seen_at = Some(now);
        self.updated_at = now;
    }

    /// Duration the incident has been (or was) open: from `started_at`
    /// to `resolved_at` if resolved, otherwise from `started_at` to
    /// `now`.
    pub fn duration(&self, now: DateTime<Utc>) -> chrono::Duration {
        self.resolved_at.unwrap_or(now) - self.started_at
    }
}
