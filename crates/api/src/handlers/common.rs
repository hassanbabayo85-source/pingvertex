use axum::http::StatusCode;
use domain::shared::ids::MonitorId;
use uuid::Uuid;

pub type ApiResult<T> = Result<T, (StatusCode, String)>;

pub fn bad_request(message: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message.into())
}

pub fn not_found(message: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, message.into())
}

pub fn internal_error(message: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, message.into())
}

pub fn parse_monitor_id(value: &str) -> ApiResult<MonitorId> {
    let id = Uuid::parse_str(value).map_err(|_| bad_request("invalid monitor id"))?;
    Ok(MonitorId(id))
}
