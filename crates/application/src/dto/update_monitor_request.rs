use domain::HttpMethod;
use domain::shared::ids::MonitorId;

/// Request for updating an existing monitor.
#[derive(Debug, Clone)]
pub struct UpdateMonitorRequest {
    pub id: MonitorId,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub ssl_check_enabled: bool,
    pub ssl_expiry_warning_days: u32,
}
