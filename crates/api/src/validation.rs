use domain::shared::ids::{MonitorId, UserId};
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
