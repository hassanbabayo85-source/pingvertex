use domain::shared::ids::{
    EscalationPolicyId, IncidentId, MaintenanceWindowId, MonitorId, NotificationChannelId, UserId,
};
use uuid::Uuid;

use crate::error::ApiError;

/// Parses a UUID string into a Uuid.
pub fn parse_uuid(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::bad_request("invalid uuid"))
}

/// Parses a monitor id string into MonitorId.
pub fn parse_monitor_id(value: &str) -> Result<MonitorId, ApiError> {
    Ok(MonitorId(parse_uuid(value)?))
}

/// Parses an incident id string into IncidentId.
pub fn parse_incident_id(value: &str) -> Result<IncidentId, ApiError> {
    Ok(IncidentId(parse_uuid(value)?))
}

/// Parses a notification channel id string into NotificationChannelId.
pub fn parse_notification_channel_id(value: &str) -> Result<NotificationChannelId, ApiError> {
    Ok(NotificationChannelId(parse_uuid(value)?))
}

/// Parses an escalation policy id string into EscalationPolicyId.
pub fn parse_escalation_policy_id(value: &str) -> Result<EscalationPolicyId, ApiError> {
    Ok(EscalationPolicyId(parse_uuid(value)?))
}

/// Parses a maintenance window id string into MaintenanceWindowId.
pub fn parse_maintenance_window_id(value: &str) -> Result<MaintenanceWindowId, ApiError> {
    Ok(MaintenanceWindowId(parse_uuid(value)?))
}

/// Parses an optional user id string into UserId.
pub fn require_user_id(value: Option<String>) -> Result<UserId, ApiError> {
    let raw = value.ok_or_else(|| ApiError::bad_request("user_id is required"))?;
    Ok(UserId(parse_uuid(&raw)?))
}

/// Validates pagination parameters.
pub fn validate_pagination(limit: u32, _offset: u32) -> Result<(), ApiError> {
    if limit == 0 || limit > 100 {
        return Err(ApiError::bad_request("limit must be between 1 and 100"));
    }
    Ok(())
}

/// Parses a status page id string into StatusPageId.
pub fn parse_status_page_id(value: &str) -> Result<domain::shared::ids::StatusPageId, ApiError> {
    Ok(domain::shared::ids::StatusPageId(parse_uuid(value)?))
}

/// Parses a proxy check job id string into ProxyCheckJobId.
pub fn parse_proxy_check_job_id(
    value: &str,
) -> Result<domain::shared::ids::ProxyCheckJobId, ApiError> {
    Ok(domain::shared::ids::ProxyCheckJobId(parse_uuid(value)?))
}
