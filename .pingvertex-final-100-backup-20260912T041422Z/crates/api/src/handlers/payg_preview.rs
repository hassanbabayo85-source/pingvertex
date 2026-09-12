use axum::{
    Json,
    extract::{Extension, State},
};
use domain::MonitorKind;
use domain::shared::types::{AuthenticatedUser, Permission};
use serde::{Deserialize, Serialize};

use application::use_cases::calculate_payg_preview::{
    CalculatePaygPreviewUseCase, PaygPreviewError, PaygPreviewInput,
};

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Deserialize)]
pub struct PaygPreviewRequest {
    pub monitors: u64,
    pub interval_seconds: u32,
    #[serde(default = "default_regions")]
    pub regions: u32,
    #[serde(default = "default_forecast_days")]
    pub forecast_days: u32,
    #[serde(default)]
    pub kind: MonitorKind,
}

fn default_regions() -> u32 {
    1
}
fn default_forecast_days() -> u32 {
    30
}

#[derive(Debug, Serialize)]
pub struct PaygPreviewResponse {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub regions: u32,
    pub forecast_days: u32,
    pub checks_per_day: u64,
    pub checks_total: u64,
    pub price_per_check_units: i64,
    pub currency: String,
    pub pricing_version: i64,
    pub estimated_cost_units: i64,
    pub estimate_label: &'static str,
}

pub async fn payg_preview(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(request): Json<PaygPreviewRequest>,
) -> Result<Json<PaygPreviewResponse>, ApiError> {
    user.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;

    // account_type is server-owned and MUST be loaded from the tenant
    // record rather than trusted from client input. Without this, a
    // company tenant would be quoted the individual (or generic) price
    // while the actual billing path — which already resolves
    // `account_type` from the tenant — would debit at the company price.
    let tenant_id = user.tenant_context().tenant_id();
    let tenant = state
        .platform_tenant_repository
        .find_by_id(tenant_id)
        .await
        .map_err(|err| {
            ApiError::internal_with_source("failed to load tenant pricing context", err)
        })?
        .ok_or_else(|| ApiError::not_found("tenant not found"))?;

    let quote = CalculatePaygPreviewUseCase::new(state.payg_pricing_repository.clone())
        .with_interval_policy_repository(state.monitor_interval_policy_repository.clone())
        .execute(PaygPreviewInput {
            account_type: Some(tenant.account_type),
            monitors: request.monitors,
            interval_seconds: request.interval_seconds,
            regions: request.regions,
            forecast_days: request.forecast_days,
            kind: request.kind,
        })
        .await
        .map_err(|err| match err {
            PaygPreviewError::Invalid(message) => ApiError::bad_request(message),
            PaygPreviewError::NoPricing => ApiError::internal("PAYG pricing unavailable"),
            PaygPreviewError::Overflow => ApiError::bad_request("forecast is too large"),
            PaygPreviewError::Repository(error) => {
                ApiError::internal_with_source("failed to load PAYG pricing", error)
            }
        })?;

    Ok(Json(PaygPreviewResponse {
        monitors: quote.monitors,
        interval_seconds: quote.interval_seconds,
        regions: quote.regions,
        forecast_days: quote.forecast_days,
        checks_per_day: quote.checks_per_day,
        checks_total: quote.checks_total,
        price_per_check_units: quote.unit_price,
        currency: quote.currency,
        pricing_version: quote.pricing_version,
        estimated_cost_units: quote.estimated_cost_units,
        estimate_label: "Estimated cost",
    }))
}
