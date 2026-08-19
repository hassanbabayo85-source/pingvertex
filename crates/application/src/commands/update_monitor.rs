use crate::dto::AssertionInput;
use domain::HttpMethod;
use domain::MonitorKind;
use domain::shared::ids::MonitorId;

#[derive(Debug, Clone)]
pub struct UpdateMonitor {
    pub id: MonitorId,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub assertions: Vec<AssertionInput>,
    /// Whether the monitor should be exposed on the unauthenticated public
    /// status page.
    pub is_public: bool,
    pub ssl_check_enabled: bool,
    pub ssl_expiry_warning_days: u32,
    /// The kind of check this monitor performs.
    pub kind: MonitorKind,
    /// Required (and only meaningful) when `kind` is `Tcp`.
    pub tcp_port: Option<u16>,
    /// Required (and only meaningful) when `kind` is `Dns`.
    pub dns_record_type: Option<String>,
    /// Optional; only meaningful when `kind` is `Dns`.
    pub dns_expected_value: Option<String>,
    /// Required (and only meaningful) when `kind` is `Heartbeat`.
    pub heartbeat_interval_seconds: Option<u32>,
    /// Required (and only meaningful) when `kind` is `Heartbeat`.
    pub heartbeat_grace_period_seconds: Option<u32>,
}
