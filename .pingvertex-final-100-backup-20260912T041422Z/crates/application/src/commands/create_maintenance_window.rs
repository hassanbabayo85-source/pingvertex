use chrono::{DateTime, Utc};
use domain::shared::ids::MonitorId;

/// Command to create a maintenance window on a monitor.
#[derive(Debug, Clone)]
pub struct CreateMaintenanceWindow {
    pub monitor_id: MonitorId,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub reason: String,
}
