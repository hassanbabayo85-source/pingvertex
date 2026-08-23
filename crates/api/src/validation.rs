use chrono::{DateTime, Utc};
use domain::CheckStatus;
use domain::shared::ids::{
    ApiKeyId, EscalationPolicyId, IncidentId, MaintenanceWindowId, MonitorId,
    NotificationChannelId, UserId,
};
use domain::shared::types::Role;
use uuid::Uuid;

use crate::error::ApiError;

pub fn parse_uuid(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::bad_request("invalid uuid"))
}

pub fn parse_monitor_id(value: &str) -> Result<MonitorId, ApiError> {
    Ok(MonitorId(parse_uuid(value)?))
}

pub fn parse_incident_id(value: &str) -> Result<IncidentId, ApiError> {
    Ok(IncidentId(parse_uuid(value)?))
}

pub fn parse_notification_channel_id(value: &str) -> Result<NotificationChannelId, ApiError> {
    Ok(NotificationChannelId(parse_uuid(value)?))
}

pub fn parse_escalation_policy_id(value: &str) -> Result<EscalationPolicyId, ApiError> {
    Ok(EscalationPolicyId(parse_uuid(value)?))
}

pub fn parse_maintenance_window_id(value: &str) -> Result<MaintenanceWindowId, ApiError> {
    Ok(MaintenanceWindowId(parse_uuid(value)?))
}

pub fn require_user_id(value: Option<String>) -> Result<UserId, ApiError> {
    let raw = value.ok_or_else(|| ApiError::bad_request("user_id is required"))?;
    Ok(UserId(parse_uuid(&raw)?))
}

pub fn validate_pagination(limit: u32, _offset: u32) -> Result<(), ApiError> {
    if limit == 0 || limit > 100 {
        return Err(ApiError::bad_request("limit must be between 1 and 100"));
    }
    Ok(())
}

pub fn parse_check_status(value: &str) -> Result<CheckStatus, ApiError> {
    match value {
        "Up" | "UP" | "up" => Ok(CheckStatus::Up),
        "Down" | "DOWN" | "down" => Ok(CheckStatus::Down),
        _ => Err(ApiError::bad_request(
            "invalid check status; expected Up or Down",
        )),
    }
}

pub fn parse_datetime(value: &str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| ApiError::bad_request("invalid datetime; expected RFC 3339"))
}

pub fn parse_status_page_id(value: &str) -> Result<domain::shared::ids::StatusPageId, ApiError> {
    Ok(domain::shared::ids::StatusPageId(parse_uuid(value)?))
}

pub fn parse_api_key_id(value: &str) -> Result<ApiKeyId, ApiError> {
    Ok(ApiKeyId(parse_uuid(value)?))
}

pub fn parse_proxy_check_job_id(
    value: &str,
) -> Result<domain::shared::ids::ProxyCheckJobId, ApiError> {
    Ok(domain::shared::ids::ProxyCheckJobId(parse_uuid(value)?))
}

pub fn parse_role(value: &str) -> Result<Role, ApiError> {
    match value {
        "Owner" => Ok(Role::Owner),
        "Admin" => Ok(Role::Admin),
        "Member" => Ok(Role::Member),
        "Viewer" => Ok(Role::Viewer),
        "Auditor" => Ok(Role::Auditor),
        _ => Err(ApiError::bad_request("invalid role")),
    }
}
