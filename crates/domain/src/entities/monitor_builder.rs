use chrono::{DateTime, Utc};

use crate::shared::ids::{MonitorId, TenantId, UserId};

use crate::{
    entities::{Monitor, monitor::CreateMonitor},
    enums::{HttpMethod, MonitorKind},
    errors::DomainError,
    value_objects::{CheckInterval, ExpectedStatus, HeartbeatToken, MonitorName, Timeout, Url},
};

#[derive(Debug)]
pub struct MonitorBuilder {
    id: MonitorId,
    user_id: UserId,
    tenant_id: TenantId,

    name: Option<MonitorName>,
    url: Option<Url>,
    method: HttpMethod,
    expected_status: Option<ExpectedStatus>,
    interval: Option<CheckInterval>,
    timeout: Option<Timeout>,

    kind: MonitorKind,
    tcp_port: Option<u16>,
    dns_record_type: Option<String>,
    dns_expected_value: Option<String>,
    heartbeat_token: Option<HeartbeatToken>,
    heartbeat_interval_seconds: Option<u32>,
    heartbeat_grace_period_seconds: Option<u32>,
    last_heartbeat_at: Option<DateTime<Utc>>,
}

impl MonitorBuilder {
    pub fn new(id: MonitorId, user_id: UserId, tenant_id: TenantId) -> Self {
        Self {
            id,
            user_id,
            tenant_id,
            name: None,
            url: None,
            method: HttpMethod::Get,
            expected_status: None,
            interval: None,
            timeout: None,
            kind: MonitorKind::Http,
            tcp_port: None,
            dns_record_type: None,
            dns_expected_value: None,
            heartbeat_token: None,
            heartbeat_interval_seconds: None,
            heartbeat_grace_period_seconds: None,
            last_heartbeat_at: None,
        }
    }

    pub fn name(mut self, value: MonitorName) -> Self {
        self.name = Some(value);
        self
    }

    pub fn url(mut self, value: Url) -> Self {
        self.url = Some(value);
        self
    }

    pub fn method(mut self, value: HttpMethod) -> Self {
        self.method = value;
        self
    }

    pub fn expected_status(mut self, value: ExpectedStatus) -> Self {
        self.expected_status = Some(value);
        self
    }

    pub fn interval(mut self, value: CheckInterval) -> Self {
        self.interval = Some(value);
        self
    }

    pub fn timeout(mut self, value: Timeout) -> Self {
        self.timeout = Some(value);
        self
    }

    pub fn kind(mut self, value: MonitorKind) -> Self {
        self.kind = value;
        self
    }

    pub fn tcp_port(mut self, value: u16) -> Self {
        self.tcp_port = Some(value);
        self
    }

    pub fn dns_record_type(mut self, value: impl Into<String>) -> Self {
        self.dns_record_type = Some(value.into());
        self
    }

    pub fn dns_expected_value(mut self, value: impl Into<String>) -> Self {
        self.dns_expected_value = Some(value.into());
        self
    }

    pub fn heartbeat_token(mut self, value: HeartbeatToken) -> Self {
        self.heartbeat_token = Some(value);
        self
    }

    pub fn heartbeat_interval_seconds(mut self, value: u32) -> Self {
        self.heartbeat_interval_seconds = Some(value);
        self
    }

    pub fn heartbeat_grace_period_seconds(mut self, value: u32) -> Self {
        self.heartbeat_grace_period_seconds = Some(value);
        self
    }

    pub fn last_heartbeat_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_heartbeat_at = Some(value);
        self
    }

    fn into_create_monitor(self) -> CreateMonitor {
        CreateMonitor {
            id: self.id,
            user_id: self.user_id,
            tenant_id: self.tenant_id,
            name: self.name.expect("Monitor name is required"),
            url: self.url.expect("URL is required").as_str().to_owned(),
            method: self.method,
            expected_status: self
                .expected_status
                .expect("Expected status is required")
                .into(),
            interval_seconds: self.interval.expect("Interval is required").into(),
            timeout_seconds: self.timeout.expect("Timeout is required").into(),
            regions: Vec::new(),
            kind: self.kind,
            tcp_port: self.tcp_port,
            dns_record_type: self.dns_record_type,
            dns_expected_value: self.dns_expected_value,
            heartbeat_token: self.heartbeat_token,
            heartbeat_interval_seconds: self.heartbeat_interval_seconds,
            heartbeat_grace_period_seconds: self.heartbeat_grace_period_seconds,
            last_heartbeat_at: self.last_heartbeat_at,
        }
    }

    /// Builds the monitor, panicking if `kind`-specific required fields are
    /// missing (e.g. a `Tcp` monitor without `tcp_port`).
    ///
    /// SAFE for every existing call site: none of them ever call `.kind(...)`,
    /// so `kind` stays at its default `Http`, which has no kind-specific
    /// required fields and therefore never fails validation. New code that
    /// sets a non-`Http` kind (e.g. the `create_monitor` use case handling
    /// user input) MUST use `try_build()` instead so validation errors
    /// surface as a `Result` rather than a panic.
    pub fn build(self) -> Monitor {
        let input = self.into_create_monitor();
        Monitor::create(input).expect(
            "MonitorBuilder::build() panicked on kind-field validation — \
             use try_build() instead when setting a non-Http kind",
        )
    }

    /// Builds the monitor, returning a `DomainError` if `kind`-specific
    /// required fields are missing. Use this whenever `kind` may be
    /// non-`Http` (i.e. whenever the value came from user input).
    pub fn try_build(self) -> Result<Monitor, DomainError> {
        let input = self.into_create_monitor();
        Monitor::create(input)
    }
}
