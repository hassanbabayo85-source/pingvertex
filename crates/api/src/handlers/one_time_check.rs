use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Deserialize)]
pub struct CreateOneTimeCheckRequest {
    pub monitor_id: Uuid,
    /// Optional client-supplied idempotency key. Never interpreted as a
    /// billing value. When omitted, the server generates one.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OneTimeCheckRegionResult {
    pub region: String,
    pub status: String,
    pub billable: bool,
    pub charge_units: i64,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OneTimeCheckResponse {
    pub id: String,
    pub request_id: String,
    pub monitor_id: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<String>,
    pub regions: Vec<OneTimeCheckRegionResult>,
}

fn build_response(
    request: domain::OneTimeCheckRequest,
    executions: Vec<domain::OneTimeCheckExecution>,
) -> OneTimeCheckResponse {
    let regions = executions
        .into_iter()
        .map(|e| OneTimeCheckRegionResult {
            region: e.region,
            status: e.status,
            billable: e.billable,
            charge_units: e.charge_units,
            error: e.error,
        })
        .collect();
    OneTimeCheckResponse {
        id: request.id.to_string(),
        request_id: request.request_id,
        monitor_id: request.monitor_id.to_string(),
        status: request.status,
        created_at: request.created_at,
        completed_at: request.completed_at,
        error: request.error,
        regions,
    }
}

pub async fn create_one_time_check(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateOneTimeCheckRequest>,
) -> Result<(StatusCode, Json<OneTimeCheckResponse>), ApiError> {
    user.require(Permission::CreateMonitor).map_err(|_| {
        ApiError::forbidden("you do not have permission to trigger one-time checks")
    })?;

    info!("API: create_one_time_check");

    let tenant_id = user.tenant_context().tenant_id();
    let monitor_id = domain::shared::ids::MonitorId(payload.monitor_id);

    let monitor = state
        .monitor_repository
        .find_by_id(monitor_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch monitor", err))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;

    if monitor.tenant_id != tenant_id {
        return Err(ApiError::not_found("monitor not found"));
    }

    if !monitor.enabled {
        return Err(ApiError::conflict("monitor is disabled"));
    }

    let tenant = state
        .platform_tenant_repository
        .find_by_id(tenant_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to load tenant status", err))?
        .ok_or_else(|| ApiError::not_found("tenant not found"))?;

    if tenant.status != domain::TenantStatus::Active {
        return Err(ApiError::forbidden("tenant is suspended"));
    }

    // Canonical one-time operations must be configured; price resolution for
    // each regional execution happens inside the metering layer at run time.
    let operation = domain::monitor_operation(monitor.kind, true);
    if !domain::PaygOperation::is_canonical(operation.as_str()) {
        return Err(ApiError::bad_request(
            "PAYG pricing is not configured for one-time checks",
        ));
    }

    let request_id = payload
        .idempotency_key
        .filter(|k| !k.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let request = state
        .scheduling_repository
        .enqueue_one_time_check(
            tenant_id,
            monitor_id,
            user.user_id(),
            &request_id,
            &monitor.regions,
        )
        .await
        .map_err(|err| ApiError::internal_with_source("failed to enqueue one-time check", err))?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(request, Vec::new())),
    ))
}

pub async fn get_one_time_check(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    axum::extract::Path(request_id): axum::extract::Path<String>,
) -> Result<Json<OneTimeCheckResponse>, ApiError> {
    user.require(Permission::ViewMonitor)
        .map_err(|_| ApiError::forbidden("you do not have permission to view one-time checks"))?;

    let tenant_id = user.tenant_context().tenant_id();
    // The monitor id placeholder is intentionally ignored by the
    // tenant-scoped lookup below; the request_id itself is globally unique
    // within the tenant.
    let request = state
        .scheduling_repository
        .get_one_time_check_by_request_id(
            tenant_id,
            domain::shared::ids::MonitorId(Uuid::nil()),
            &request_id,
        )
        .await
        .map_err(|err| ApiError::internal_with_source("failed to get one-time check", err))?
        .ok_or_else(|| ApiError::not_found("one-time check not found"))?;

    let executions = state
        .scheduling_repository
        .get_one_time_check_executions(request.id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to load one-time executions", err))?;

    Ok(Json(build_response(request, executions)))
}
