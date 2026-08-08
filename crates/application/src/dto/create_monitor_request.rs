use domain::HttpMethod;

/// Input data required to create a monitor.
#[derive(Debug, Clone)]
pub struct CreateMonitorRequest {
    pub user_id: domain::shared::ids::UserId,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
}
