use axum::{
    Json,
    extract::{Extension, Path, State},
};
use domain::shared::types::{AuthenticatedUser, Permission};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

async fn control_job(
    state: AppState,
    user: AuthenticatedUser,
    job_id: Uuid,
    pause: bool,
) -> Result<Json<application::dto::BulkMonitorImportJob>, ApiError> {
    user.require(Permission::PauseMonitor)
        .map_err(|e| ApiError::forbidden(e.to_string()))?;

    let repository = state
        .bulk_monitor_import_repository
        .as_ref()
        .ok_or_else(|| ApiError::internal("bulk monitor import is not configured"))?;

    let tenant_id = user.tenant_context().tenant_id().0;
    let changed = if pause {
        repository.pause_job(job_id, tenant_id).await
    } else {
        repository.resume_job(job_id, tenant_id).await
    }
    .map_err(|error| ApiError::internal_with_source("failed to update bulk import job", error))?;

    if !changed {
        let job = repository
            .find_job_for_tenant(job_id, tenant_id)
            .await
            .map_err(|error| {
                ApiError::internal_with_source("failed to load bulk import job", error)
            })?
            .ok_or_else(|| ApiError::not_found("bulk import job not found"))?;

        return Ok(Json(job));
    }

    let action = if pause {
        domain::AuditAction::BulkImportPaused
    } else {
        domain::AuditAction::BulkImportResumed
    };
    let entry = domain::AuditLogEntry {
        id: domain::AuditLogEntryId::new(),
        actor_user_id: user.user_id(),
        tenant_id: Some(user.tenant_context().tenant_id()),
        action,
        target_id: format!("bulk-import:{job_id}"),
        metadata: serde_json::json!({
            "job_id": job_id,
            "action": if pause { "pause" } else { "resume" },
        }),
        created_at: chrono::Utc::now(),
    };
    state
        .audit_log_entry_repository
        .create(&entry)
        .await
        .map_err(|error| {
            ApiError::internal_with_source("failed to record bulk import audit", error)
        })?;

    let job = repository
        .find_job_for_tenant(job_id, tenant_id)
        .await
        .map_err(|error| ApiError::internal_with_source("failed to load bulk import job", error))?
        .ok_or_else(|| ApiError::not_found("bulk import job not found"))?;

    Ok(Json(job))
}

pub async fn pause_bulk_monitor_import(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<application::dto::BulkMonitorImportJob>, ApiError> {
    control_job(state, user, job_id, true).await
}

pub async fn resume_bulk_monitor_import(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<application::dto::BulkMonitorImportJob>, ApiError> {
    control_job(state, user, job_id, false).await
}
