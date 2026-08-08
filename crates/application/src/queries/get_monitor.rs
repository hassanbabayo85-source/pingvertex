use domain::shared::ids::{MonitorId, UserId};

/// Query for retrieving a monitor by its ID.
#[derive(Debug, Clone, Copy)]
pub struct GetMonitor {
    pub id: MonitorId,
    pub user_id: UserId,
}
