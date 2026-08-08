use domain::HttpMethod;
use domain::shared::ids::{MonitorId, UserId};

#[derive(Debug, Clone)]
pub struct UpdateMonitor {
    pub id: MonitorId,
    pub user_id: UserId,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
}
