use axum::{
    Json,
    extract::{Extension, State},
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
    pub affected: u64,
    pub action: BulkMonitorAction,
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
    Json(request): Json<BulkMonitorActionRequest>,
) -> Result<Json<BulkMonitorActionResponse>, ApiError> {
    user.require(Permission::PauseMonitor)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;

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

    let enabled = matches!(
        request.action,
        BulkMonitorAction::Enable | BulkMonitorAction::Resume
    );
    let audit_action = match request.action {
        BulkMonitorAction::Enable => domain::AuditAction::BulkMonitorEnabled,
        BulkMonitorAction::Disable => domain::AuditAction::BulkMonitorDisabled,
        BulkMonitorAction::Pause => domain::AuditAction::BulkMonitorPaused,
        BulkMonitorAction::Resume => domain::AuditAction::BulkMonitorResumed,
    };
    let tenant_id = user.tenant_context().tenant_id();
    let affected = state
        .monitor_repository
        .set_enabled_for_tenant(tenant_id, ids.as_deref(), enabled)
        .await
        .map_err(|e| ApiError::internal_with_source("bulk monitor action failed", e))?;

    let entry = domain::AuditLogEntry {
        id: domain::AuditLogEntryId::new(),
        actor_user_id: user.user_id(),
        tenant_id: Some(tenant_id),
        action: audit_action,
        target_id: if request.all {
            format!("tenant:{}", tenant_id.0)
        } else {
            "bulk".to_owned()
        },
        metadata: serde_json::json!({
            "bulk": true,
            "action": request.action,
            "scope": if request.all { "all" } else { "selected" },
            "affected": affected,
            "reason": reason,
        }),
        created_at: chrono::Utc::now(),
    };
    state
        .audit_log_entry_repository
        .create(&entry)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to record bulk monitor audit", e))?;

    Ok(Json(BulkMonitorActionResponse {
        affected,
        action: request.action,
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
