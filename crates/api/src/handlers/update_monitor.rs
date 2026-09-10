use axum::{
    Json,
    extract::{Extension, Path, State},
};
use tracing::info;

use application::{commands::UpdateMonitor, use_cases::UpdateMonitorUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::{MonitorResponse, MonitorWriteRequest},
    state::AppState,
    validation::parse_monitor_id,
};

pub async fn update_monitor(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(body): Json<MonitorWriteRequest>,
) -> Result<Json<MonitorResponse>, ApiError> {
    info!("API: update_monitor");

    let monitor_id = parse_monitor_id(&id)?;

    let command = UpdateMonitor {
        id: monitor_id,
        name: body.name,
        url: body.url,
        method: body.method,
        expected_status: body.expected_status,
        interval_seconds: body.interval_seconds,
        timeout_seconds: body.timeout_seconds,
        assertions: body.assertions,
        is_public: body.is_public,
        ssl_check_enabled: body.ssl_check_enabled,
        ssl_expiry_warning_days: body.ssl_expiry_warning_days,
        kind: body.kind,
        tcp_port: body.tcp_port,
        dns_record_type: body.dns_record_type,
        dns_expected_value: body.dns_expected_value,
        heartbeat_interval_seconds: body.heartbeat_interval_seconds,
        heartbeat_grace_period_seconds: body.heartbeat_grace_period_seconds,
    };

    let use_case = UpdateMonitorUseCase::new(
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    )
    .with_wallet_repository(state.wallet_repository.clone())
    .with_interval_policy_repository(state.monitor_interval_policy_repository.clone())
    .with_pricing_config_repository(state.pricing_config_repository.clone());

    let monitor = use_case
        .execute(command, authenticated_user)
        .await
        .map_err(|err| {
            if err
                .downcast_ref::<domain::shared::types::PermissionDenied>()
                .is_some()
            {
                return ApiError::forbidden(err.to_string());
            }

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
                ApiError::internal_with_source("failed to update monitor", err)
            }
        })?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    Ok(Json(MonitorResponse::from(monitor)))
}
