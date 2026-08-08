use domain::shared::ids::{MonitorId, UserId};

#[derive(Debug, Clone, Copy)]
pub struct DeleteMonitor {
    pub id: MonitorId,
    pub user_id: UserId,
}
