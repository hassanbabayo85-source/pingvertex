use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::HeaderMap,
};
use chrono::Utc;
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{EscalationPolicy, EscalationStep};

use crate::{
    error::ApiError,
    handlers::pagination::{PaginationQuery, pagination_headers},
    state::AppState,
    validation::{parse_escalation_policy_id, parse_monitor_id, parse_notification_channel_id},
};

#[derive(Debug, serde::Serialize)]
pub struct MaintenanceWindowResponse {
    pub id: String,
    pub monitor_id: String,
    pub starts_at: chrono::DateTime<Utc>,
    pub ends_at: chrono::DateTime<Utc>,
    pub reason: String,
    pub created_at: chrono::DateTime<Utc>,
}
impl From<domain::MaintenanceWindow> for MaintenanceWindowResponse {
    fn from(v: domain::MaintenanceWindow) -> Self {
        Self {
            id: v.id.0.to_string(),
            monitor_id: v.monitor_id.0.to_string(),
            starts_at: v.starts_at,
            ends_at: v.ends_at,
            reason: v.reason,
            created_at: v.created_at,
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct EscalationStepResponse {
    pub delay_minutes: u32,
    pub channel_id: String,
}
#[derive(Debug, serde::Serialize)]
pub struct EscalationPolicyResponse {
    pub id: String,
    pub monitor_id: Option<String>,
    pub steps: Vec<EscalationStepResponse>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}
impl From<domain::EscalationPolicy> for EscalationPolicyResponse {
    fn from(v: domain::EscalationPolicy) -> Self {
        Self {
            id: v.id.0.to_string(),
            monitor_id: v.monitor_id.map(|id| id.0.to_string()),
            steps: v
                .steps
                .into_iter()
                .map(|s| EscalationStepResponse {
                    delay_minutes: s.delay_minutes,
                    channel_id: s.channel_id.0.to_string(),
                })
                .collect(),
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateEscalationPolicyRequest {
    pub monitor_id: Option<String>,
    pub steps: Vec<crate::handlers::feature_types::EscalationStepInput>,
}

/// `GET /api/v1/monitors/{monitor_id}/maintenance-windows`
///
/// PART 8 — cursor-paginated. Response body is the same JSON array as
/// before; a next-page cursor (when present) is returned via the
/// `X-Next-Cursor` response header.
pub async fn list_maintenance_windows(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(monitor_id): Path<String>,
    Query(query): Query<PaginationQuery>,
) -> Result<(HeaderMap, Json<Vec<MaintenanceWindowResponse>>), ApiError> {
    let monitor_id = parse_monitor_id(&monitor_id)?;
    let monitor = state
        .monitor_repository
        .find_by_id(monitor_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch monitor", e))?
        .ok_or_else(|| ApiError::not_found("monitor not found"))?;
    if !AuthorizationPolicy::same_tenant(&user.tenant_context(), monitor.tenant_id()) {
        return Err(ApiError::not_found("monitor not found"));
    }
    let page = state
        .maintenance_window_repository
        .list_by_monitor_paginated(monitor_id, query.into_page_request())
        .await
        .map_err(|e| ApiError::internal_with_source("failed to list maintenance windows", e))?;

    let headers = pagination_headers(page.next_cursor)?;
    let body: Vec<MaintenanceWindowResponse> = page
        .items
        .into_iter()
        .map(MaintenanceWindowResponse::from)
        .collect();
    Ok((headers, Json(body)))
}

/// `GET /api/v1/escalation-policies`
///
/// PART 8 — cursor-paginated. Response body is the same JSON array as
/// before; a next-page cursor (when present) is returned via the
/// `X-Next-Cursor` response header.
pub async fn list_escalation_policies(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<PaginationQuery>,
) -> Result<(HeaderMap, Json<Vec<EscalationPolicyResponse>>), ApiError> {
    let page = state
        .escalation_policy_repository
        .list_for_tenant_paginated(user.tenant_context().tenant_id(), query.into_page_request())
        .await
        .map_err(|e| ApiError::internal_with_source("failed to list escalation policies", e))?;

    let headers = pagination_headers(page.next_cursor)?;
    let body: Vec<EscalationPolicyResponse> = page
        .items
        .into_iter()
        .map(EscalationPolicyResponse::from)
        .collect();
    Ok((headers, Json(body)))
}

pub async fn update_escalation_policy(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateEscalationPolicyRequest>,
) -> Result<Json<EscalationPolicyResponse>, ApiError> {
    if !user.can(Permission::CreateMonitor) {
        return Err(ApiError::forbidden(
            "you do not have permission to update escalation policies",
        ));
    }
    let id = parse_escalation_policy_id(&id)?;
    let existing = state
        .escalation_policy_repository
        .find_by_id(id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch escalation policy", e))?
        .ok_or_else(|| ApiError::not_found("escalation policy not found"))?;
    if !AuthorizationPolicy::same_tenant(&user.tenant_context(), existing.tenant_id()) {
        return Err(ApiError::not_found("escalation policy not found"));
    }
    let monitor_id = match payload.monitor_id {
        Some(v) => {
            let mid = parse_monitor_id(&v)?;
            let monitor = state
                .monitor_repository
                .find_by_id(mid)
                .await
                .map_err(|e| ApiError::internal_with_source("failed to fetch monitor", e))?
                .ok_or_else(|| ApiError::not_found("monitor not found"))?;
            if !AuthorizationPolicy::same_tenant(&user.tenant_context(), monitor.tenant_id()) {
                return Err(ApiError::not_found("monitor not found"));
            }
            Some(mid)
        }
        None => None,
    };
    if payload.steps.is_empty()
        || payload.steps.len() > domain::entities::escalation_policy::MAX_ESCALATION_STEPS
    {
        return Err(ApiError::bad_request(
            "escalation policy must contain between 1 and 5 steps",
        ));
    }
    let mut steps = Vec::with_capacity(payload.steps.len());
    for input in payload.steps {
        let channel_id = parse_notification_channel_id(&input.channel_id)?;
        let channel = state
            .notification_channel_repository
            .find_by_id(channel_id)
            .await
            .map_err(|e| ApiError::internal_with_source("failed to fetch notification channel", e))?
            .ok_or_else(|| ApiError::not_found("notification channel not found"))?;
        if !AuthorizationPolicy::same_tenant(&user.tenant_context(), channel.tenant_id())
            || (monitor_id.is_some() && channel.monitor_id != monitor_id.unwrap())
        {
            return Err(ApiError::not_found("notification channel not found"));
        }
        steps.push(EscalationStep {
            delay_minutes: input.delay_minutes,
            channel_id,
        });
    }
    let updated = EscalationPolicy {
        id: existing.id,
        user_id: existing.user_id,
        tenant_id: existing.tenant_id,
        monitor_id,
        steps,
        created_at: existing.created_at,
        updated_at: Utc::now(),
    };
    state
        .escalation_policy_repository
        .update(&updated)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to update escalation policy", e))?;
    Ok(Json(updated.into()))
}
