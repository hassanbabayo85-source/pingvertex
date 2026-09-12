use axum::{
    Json,
    extract::{Extension, State},
    http::HeaderMap,
};
use domain::shared::{
    ids::MonitorId,
    types::{AuthenticatedUser, Permission},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

const MAX_SELECTED_MONITORS: usize = 500;
const MAX_AUDIT_REASON_LEN: usize = 500;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkMonitorAction {
    Enable,
    Disable,
    Pause,
    Resume,
}

#[derive(Debug, Deserialize)]
pub struct BulkMonitorActionRequest {
    pub action: BulkMonitorAction,
    #[serde(default)]
    pub monitor_ids: Vec<String>,
    #[serde(default)]
    pub all: bool,
    /// Optional operator-supplied reason retained in the append-only audit metadata.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BulkMonitorActionResponse {
    pub job_id: Uuid,
    pub status: String,
    pub affected: u64,
    pub action: BulkMonitorAction,
}

fn required_permission(action: BulkMonitorAction) -> Permission {
    match action {
        BulkMonitorAction::Enable | BulkMonitorAction::Disable => Permission::PauseMonitor,
        BulkMonitorAction::Pause | BulkMonitorAction::Resume => Permission::PauseMonitor,
    }
}

fn normalize_reason(reason: Option<&str>) -> Result<Option<String>, ApiError> {
    let reason = reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    if reason
        .as_ref()
        .is_some_and(|value| value.chars().count() > MAX_AUDIT_REASON_LEN)
    {
        return Err(ApiError::bad_request(format!(
            "reason must be at most {MAX_AUDIT_REASON_LEN} characters"
        )));
    }

    Ok(reason)
}

pub async fn bulk_monitor_action(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Json(request): Json<BulkMonitorActionRequest>,
) -> Result<Json<BulkMonitorActionResponse>, ApiError> {
    user.require(required_permission(request.action))
        .map_err(|e| ApiError::forbidden(e.to_string()))?;

    let tenant_id = user.tenant_context().tenant_id();
    let tenant = state
        .platform_tenant_repository
        .find_by_id(tenant_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load tenant status", e))?
        .ok_or_else(|| ApiError::not_found("tenant not found"))?;
    if tenant.status != domain::TenantStatus::Active {
        return Err(ApiError::forbidden("tenant is suspended"));
    }

    if request.all && !request.monitor_ids.is_empty() {
        return Err(ApiError::bad_request(
            "choose either all=true or monitor_ids, not both",
        ));
    }
    if !request.all && request.monitor_ids.is_empty() {
        return Err(ApiError::bad_request(
            "monitor_ids is required when all is false",
        ));
    }
    if request.monitor_ids.len() > MAX_SELECTED_MONITORS {
        return Err(ApiError::bad_request(format!(
            "a bulk request may contain at most {MAX_SELECTED_MONITORS} monitor ids"
        )));
    }

    let reason = normalize_reason(request.reason.as_deref())?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    if idempotency_key.as_ref().is_some_and(|v| v.len() > 128) {
        return Err(ApiError::bad_request(
            "Idempotency-Key must be at most 128 characters",
        ));
    }

    let ids = if request.all {
        None
    } else {
        let mut parsed = Vec::with_capacity(request.monitor_ids.len());
        for raw in &request.monitor_ids {
            let id = Uuid::parse_str(raw.trim())
                .map_err(|_| ApiError::bad_request("invalid monitor id"))?;
            parsed.push(MonitorId(id));
        }
        parsed.sort_by_key(|id| id.0);
        parsed.dedup_by_key(|id| id.0);
        Some(parsed)
    };

    let action = match request.action {
        BulkMonitorAction::Enable => "enable",
        BulkMonitorAction::Disable => "disable",
        BulkMonitorAction::Pause => "pause",
        BulkMonitorAction::Resume => "resume",
    };
    let repo = state
        .bulk_monitor_action_repository
        .as_ref()
        .ok_or_else(|| ApiError::internal("bulk monitor action queue is not configured"))?;
    let job = repo
        .create(
            tenant_id.0,
            user.user_id().0,
            action,
            ids.as_ref()
                .map(|v| v.iter().map(|id| id.0).collect::<Vec<_>>())
                .as_deref(),
            request.all,
            reason.as_deref(),
            idempotency_key.as_deref(),
        )
        .await
        .map_err(|e| ApiError::internal_with_source("failed to enqueue bulk monitor action", e))?;

    Ok(Json(BulkMonitorActionResponse {
        job_id: job.id,
        status: job.status,
        affected: job.affected.unwrap_or(0),
        action: request.action,
    }))
}

pub async fn get_bulk_monitor_action_job(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    axum::extract::Path(job_id): axum::extract::Path<Uuid>,
) -> Result<Json<BulkMonitorActionResponse>, ApiError> {
    user.require(Permission::PauseMonitor)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    let tenant_id = user.tenant_context().tenant_id();
    let repo = state
        .bulk_monitor_action_repository
        .as_ref()
        .ok_or_else(|| ApiError::internal("bulk monitor action queue is not configured"))?;
    let job = repo
        .find_for_tenant(job_id, tenant_id.0)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load bulk monitor action job", e))?
        .ok_or_else(|| ApiError::not_found("bulk monitor action job not found"))?;
    let action = match job.action.as_str() {
        "enable" => BulkMonitorAction::Enable,
        "disable" => BulkMonitorAction::Disable,
        "pause" => BulkMonitorAction::Pause,
        "resume" => BulkMonitorAction::Resume,
        _ => return Err(ApiError::internal("invalid bulk monitor action job")),
    };
    Ok(Json(BulkMonitorActionResponse {
        job_id: job.id,
        status: job.status,
        affected: job.affected.unwrap_or(0),
        action,
    }))
}

#[cfg(test)]
mod tests {
    use super::normalize_reason;

    #[test]
    fn normalizes_blank_reason_to_none() {
        assert_eq!(normalize_reason(Some("  ")).unwrap(), None);
    }

    #[test]
    fn trims_reason() {
        assert_eq!(
            normalize_reason(Some("  maintenance window  ")).unwrap(),
            Some("maintenance window".to_owned())
        );
    }

    #[test]
    fn rejects_reason_over_500_characters() {
        let reason = "x".repeat(501);
        assert!(normalize_reason(Some(&reason)).is_err());
    }
}
