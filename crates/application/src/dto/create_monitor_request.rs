use crate::dto::AssertionInput;
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
    pub assertions: Vec<AssertionInput>,
    /// Whether the monitor should be exposed on the unauthenticated public
    /// status page. Defaults to `false` if omitted by the caller.
    pub is_public: bool,
    pub ssl_check_enabled: bool,
    pub ssl_expiry_warning_days: u32,
}
