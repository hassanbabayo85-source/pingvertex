//! GAP 3 Kashi D + C: exposes the tenant's monthly fair-use alert
//! allowance and their effective history-retention setting.

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

    // GAP 3 Kashi C — history retention.
    /// The effective retention the retention worker will apply for this
    /// tenant. `tenant_retention_settings.retention_days` when the tenant
    /// has an explicit row, otherwise the pricing config's
    /// `default_retention_days`.
    pub retention_days: u32,
    /// The hard upper bound (from `pricing_configs.max_retention_days`).
    pub max_retention_days: u32,
    /// The value used when the tenant has no explicit row (from
    /// `pricing_configs.default_retention_days`).
    pub default_retention_days: u32,
    /// How the current `retention_days` was set:
    /// `"default"`, `"legacy"`, or `"addon"`.
    pub retention_source: String,
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

    // Retention resolution needs the active pricing config for the bounds.
    let pricing = s
        .pricing_config_repository
        .find_active_config()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load pricing config", e))?
        .ok_or_else(|| ApiError::internal("active pricing config is missing"))?;

    let retention_row = s
        .tenant_retention_settings_repository
        .get(tenant_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load retention settings", e))?;

    let (retention_days, retention_source) = match retention_row {
        Some(r) => (
            r.retention_days.min(pricing.max_retention_days).max(1),
            r.source.as_str().to_owned(),
        ),
        None => (pricing.default_retention_days, "default".to_owned()),
    };

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
        retention_days,
        max_retention_days: pricing.max_retention_days,
        default_retention_days: pricing.default_retention_days,
        retention_source,
    }))
}
