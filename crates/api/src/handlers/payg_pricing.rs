use crate::{error::ApiError, state::AppState};
use application::ports::MonitorIntervalPolicyRepository;
use application::queries::ResolveEntitlements;
use application::use_cases::ResolveEntitlementsUseCase;
use application::use_cases::calculate_payg_preview::{
    MAX_PREVIEW_FORECAST_DAYS, MAX_PREVIEW_REGIONS,
};
use axum::{
    Json,
    extract::{Extension, State},
};
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PAYGPricingResponse {
    pub operation: String,
    pub unit_name: String,
    pub unit_price: i64,
    pub currency: String,
    pub version: i64,
    pub effective_from: chrono::DateTime<chrono::Utc>,
}

/// The tenant's current free-tier entitlement, resolved by the same
/// `ResolveEntitlementsUseCase` the runtime billing and scheduler paths
/// use. Changing the DB pricing config is enough to change what the UI
/// shows.
#[derive(Debug, Serialize)]
pub struct FreeTierResponse {
    pub max_monitors: u32,
    pub min_check_interval_seconds: u32,
    pub retention_days: u32,
}

/// Server-owned limits the frontend uses to bound its client-side
/// validation and `<input min/max>` attributes. Exposing them here keeps
/// the Rust use case as the single source of truth — the UI must never
/// hardcode `30`, `32`, or `3650`.
#[derive(Debug, Serialize)]
pub struct PreviewLimitsResponse {
    pub max_regions: u32,
    pub max_forecast_days: u32,
    pub min_interval_seconds_http: u32,
    pub min_interval_seconds_tcp: u32,
    pub min_interval_seconds_dns: u32,
    pub min_interval_seconds_heartbeat: u32,
    pub min_interval_seconds_flow: u32,
}

#[derive(Debug, Serialize)]
pub struct PaygPricingPage {
    pub prices: Vec<PAYGPricingResponse>,
    pub free_tier: FreeTierResponse,
    /// Minimum wallet top-up in micro-USD. Sourced from
    /// `domain::MIN_PAYG_TOP_UP_UNITS` so the UI never hardcodes "$5".
    pub payg_min_top_up_units: i64,
    pub preview_limits: PreviewLimitsResponse,
}

pub async fn payg_pricing(
    State(s): State<AppState>,
    Extension(u): Extension<AuthenticatedUser>,
) -> Result<Json<PaygPricingPage>, ApiError> {
    u.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;

    let rows = s
        .payg_pricing_repository
        .list_active()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load PAYG pricing", e))?;

    let tenant_id = u.tenant_context().tenant_id();
    let plan = ResolveEntitlementsUseCase::new(s.subscription_repository.clone())
        .with_pricing_config_repository(s.pricing_config_repository.clone())
        .execute(ResolveEntitlements { tenant_id })
        .await
        .map_err(|e| ApiError::internal_with_source("failed to resolve tenant entitlement", e))?;

    let policy_repo = &s.monitor_interval_policy_repository;
    let min_interval_seconds_http = policy_repo
        .minimum_interval_seconds(domain::MonitorKind::Http)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load HTTP interval policy", e))?;
    let min_interval_seconds_tcp = policy_repo
        .minimum_interval_seconds(domain::MonitorKind::Tcp)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load TCP interval policy", e))?;
    let min_interval_seconds_dns = policy_repo
        .minimum_interval_seconds(domain::MonitorKind::Dns)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load DNS interval policy", e))?;
    let min_interval_seconds_heartbeat = policy_repo
        .minimum_interval_seconds(domain::MonitorKind::Heartbeat)
        .await
        .map_err(|e| {
            ApiError::internal_with_source("failed to load heartbeat interval policy", e)
        })?;
    let min_interval_seconds_flow = policy_repo
        .minimum_interval_seconds(domain::MonitorKind::Flow)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load Flow interval policy", e))?;

    Ok(Json(PaygPricingPage {
        prices: rows
            .into_iter()
            .map(|p| PAYGPricingResponse {
                operation: p.operation,
                unit_name: p.unit_name,
                unit_price: p.unit_price,
                currency: p.currency,
                version: p.version,
                effective_from: p.effective_from,
            })
            .collect(),
        free_tier: FreeTierResponse {
            max_monitors: plan.max_monitors,
            min_check_interval_seconds: plan.min_check_interval_seconds,
            retention_days: plan.history_retention_days,
        },
        payg_min_top_up_units: domain::MIN_PAYG_TOP_UP_UNITS,
        preview_limits: PreviewLimitsResponse {
            max_regions: MAX_PREVIEW_REGIONS,
            max_forecast_days: MAX_PREVIEW_FORECAST_DAYS,
            min_interval_seconds_http,
            min_interval_seconds_tcp,
            min_interval_seconds_dns,
            min_interval_seconds_heartbeat,
            min_interval_seconds_flow,
        },
    }))
}
