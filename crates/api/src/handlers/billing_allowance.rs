//! GAP 3 Kashi D: exposes the tenant's monthly fair-use alert allowance.

use crate::{error::ApiError, state::AppState};
use axum::{
    Json,
    extract::{Extension, State},
};
use chrono::{DateTime, Utc};
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AllowanceResponse {
    pub cycle_started_at: DateTime<Utc>,
    pub cycle_ends_at: DateTime<Utc>,
    pub sms_limit: i32,
    pub sms_used: i32,
    pub sms_remaining: i32,
    pub voice_limit: i32,
    pub voice_used: i32,
    pub phone_numbers_limit: i32,
    pub phone_numbers_used: i32,
    /// The monitor count this cycle's allowance is based on. Frozen at
    /// cycle start; not recomputed on read (only on reset).
    pub enabled_monitors: u64,
}

pub async fn get_allowance(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
) -> Result<Json<AllowanceResponse>, ApiError> {
    u.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;

    let tenant_id = u.tenant_context().tenant_id();
    let now = Utc::now();

    let enabled_monitors = s
        .monitor_repository
        .count_enabled_for_tenant(tenant_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to count monitors", e))?;

    let allowance = s
        .tenant_alert_allowance_repository
        .current_cycle(tenant_id, enabled_monitors, now)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load allowance", e))?;

    Ok(Json(AllowanceResponse {
        cycle_started_at: allowance.cycle_started_at,
        cycle_ends_at: allowance.cycle_ends_at,
        sms_limit: allowance.sms_limit,
        sms_used: allowance.sms_used,
        sms_remaining: (allowance.sms_limit - allowance.sms_used).max(0),
        voice_limit: allowance.voice_limit,
        voice_used: allowance.voice_used,
        phone_numbers_limit: allowance.phone_numbers_limit,
        phone_numbers_used: allowance.phone_numbers_used,
        enabled_monitors,
    }))
}
