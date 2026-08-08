use chrono::{DateTime, Utc};
use domain::shared::ids::UserId;
use domain::{HttpMethod, Monitor};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JSON representation of a monitor returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Monitor> for MonitorResponse {
    fn from(monitor: Monitor) -> Self {
        Self {
            id: monitor.id.0.to_string(),
            user_id: monitor.user_id.0.to_string(),
            name: monitor.name.as_str().to_owned(),
            url: monitor.url,
            method: monitor.method,
            expected_status: monitor.expected_status,
            interval_seconds: monitor.interval_seconds,
            timeout_seconds: monitor.timeout_seconds,
            enabled: monitor.enabled,
            created_at: monitor.created_at,
            updated_at: monitor.updated_at,
        }
    }
}

/// Shared request body shape for create/update endpoints.
#[derive(Debug, Clone, Deserialize)]
pub struct MonitorWriteRequest {
    pub user_id: Option<String>,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
}

impl MonitorWriteRequest {
    pub fn parse_user_id(&self) -> Result<UserId, String> {
        let raw = self
            .user_id
            .as_ref()
            .ok_or_else(|| "user_id is required".to_string())?;
        let id = Uuid::parse_str(raw).map_err(|_| "invalid user_id".to_string())?;
        Ok(UserId(id))
    }
}
