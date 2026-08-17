use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{MaintenanceWindowId, MonitorId};

/// Tsawon lokaci inda monitor zai iya kasawa ba tare da an tura
/// notification ba (amma har yanzu ana rubuta incident a database).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceWindow {
    pub id: MaintenanceWindowId,
    pub monitor_id: MonitorId,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

impl MaintenanceWindow {
    /// Max tsawon maintenance window: kwana 7.
    pub const MAX_DURATION_DAYS: i64 = 7;
    pub const MAX_REASON_LENGTH: usize = 500;

    pub fn new(
        id: MaintenanceWindowId,
        monitor_id: MonitorId,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
        reason: String,
    ) -> Result<Self, DomainError> {
        // 1. Kada reason ya wuce MAX_REASON_LENGTH
        if reason.len() > Self::MAX_REASON_LENGTH {
            return Err(DomainError::InvalidMaintenanceReason(format!(
                "reason must be at most {} characters",
                Self::MAX_REASON_LENGTH
            )));
        }

        // 2. Kada ends_at ya kasance kafin starts_at
        if ends_at <= starts_at {
            return Err(DomainError::InvalidMaintenanceWindow(
                "ends_at must be after starts_at".to_string(),
            ));
        }

        // 3. Dole duration bai wuce MAX_DURATION_DAYS ba
        let duration = ends_at - starts_at;
        if duration > Duration::days(Self::MAX_DURATION_DAYS) {
            return Err(DomainError::InvalidMaintenanceWindow(format!(
                "maintenance window cannot exceed {} days",
                Self::MAX_DURATION_DAYS
            )));
        }

        Ok(Self {
            id,
            monitor_id,
            starts_at,
            ends_at,
            reason,
            created_at: Utc::now(),
        })
    }

    /// Idan lokacin yanzu yana cikin wannan window.
    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.starts_at && now <= self.ends_at
    }
}
