use crate::shared::ids::{MonitorId, UserId};

use crate::{
    entities::{Monitor, monitor::CreateMonitor},
    enums::HttpMethod,
    value_objects::{CheckInterval, ExpectedStatus, MonitorName, Timeout, Url},
};

#[derive(Debug)]
pub struct MonitorBuilder {
    id: MonitorId,
    user_id: UserId,

    name: Option<MonitorName>,
    url: Option<Url>,
    method: HttpMethod,
    expected_status: Option<ExpectedStatus>,
    interval: Option<CheckInterval>,
    timeout: Option<Timeout>,
}

impl MonitorBuilder {
    pub fn new(id: MonitorId, user_id: UserId) -> Self {
        Self {
            id,
            user_id,
            name: None,
            url: None,
            method: HttpMethod::Get,
            expected_status: None,
            interval: None,
            timeout: None,
        }
    }
    // ... (sauran code ɗin daga nan zuwa ƙasa babu canji, wato fun name(), url(), method(), da build() su zauna yadda suke)
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

    pub fn build(self) -> Monitor {
        let input = CreateMonitor {
            id: self.id,
            user_id: self.user_id,
            name: self.name.expect("Monitor name is required"),
            url: self.url.expect("URL is required").as_str().to_owned(),
            method: self.method,
            expected_status: self
                .expected_status
                .expect("Expected status is required")
                .into(),
            interval_seconds: self.interval.expect("Interval is required").into(),
            timeout_seconds: self.timeout.expect("Timeout is required").into(),
        };

        Monitor::create(input)
    }
}
