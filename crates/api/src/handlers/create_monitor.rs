use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use tracing::info;

use application::{
    dto::CreateMonitorRequest, use_cases::CreateMonitorUseCase,
    use_cases::create_monitor::CreateMonitorError,
};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::{MonitorCreateResponse, MonitorWriteRequest},
    state::AppState,
};

/// Creates a monitor through the API.
pub async fn create_monitor(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<MonitorWriteRequest>,
) -> Result<(StatusCode, Json<MonitorCreateResponse>), ApiError> {
    info!("API: create_monitor");

    let request = CreateMonitorRequest {
        name: payload.name,
        url: payload.url,
        method: payload.method,
        expected_status: payload.expected_status,
        interval_seconds: payload.interval_seconds,
        timeout_seconds: payload.timeout_seconds,
        assertions: payload.assertions,
        is_public: payload.is_public,
        ssl_check_enabled: payload.ssl_check_enabled,
        ssl_expiry_warning_days: payload.ssl_expiry_warning_days,
        kind: payload.kind,
        tcp_port: payload.tcp_port,
        dns_record_type: payload.dns_record_type,
        dns_expected_value: payload.dns_expected_value,
        heartbeat_interval_seconds: payload.heartbeat_interval_seconds,
        heartbeat_grace_period_seconds: payload.heartbeat_grace_period_seconds,
    };

    let use_case = CreateMonitorUseCase::new(
        state.monitor_repository.clone(),
        state.id_generator,
        state.audit_log_entry_repository.clone(),
    )
    .with_entitlements(state.subscription_repository.clone());

    let monitor = use_case
        .execute(request, authenticated_user)
        .await
        .map_err(|err| match err {
            CreateMonitorError::Forbidden => {
                ApiError::forbidden("you do not have permission to create monitors")
            }
            CreateMonitorError::QuotaExceeded { limit } => ApiError::too_many_requests(format!(
                "monitor limit reached: your plan allows at most {limit} monitors"
            )),
            CreateMonitorError::TooManyAssertions { limit } => ApiError::bad_request(format!(
                "too many assertions: a monitor allows at most {limit} assertions"
            )),
            CreateMonitorError::IntervalBelowPlanMinimum { min } => ApiError::bad_request(format!(
                "interval must be at least {min} seconds on your current plan"
            )),
            CreateMonitorError::Other(err) => {
                let msg = err.to_string();
                let is_validation_error = msg.contains("invalid")
                    || msg.contains("cannot")
                    || msg.contains("must")
                    || msg.contains("required")
                    || msg.contains("interval")
                    || msg.contains("timeout")
                    || msg.contains("expected")
                    || msg.contains("url")
                    || msg.contains("host")
                    || msg.contains("dns")
                    || msg.contains("heartbeat");

                if is_validation_error {
                    ApiError::bad_request(msg)
                } else {
                    ApiError::internal_with_source("failed to create monitor", err)
                }
            }
        })?;

    Ok((
        StatusCode::CREATED,
        Json(MonitorCreateResponse::from_created(monitor)),
    ))
}
