use crate::{error::ApiError, state::AppState};
use application::use_cases::calculate_pricing_quote::CalculatePricingQuoteUseCase;
use axum::{Json, extract::{Extension, State}};
use domain::pricing::PricingInput;

#[derive(Debug, serde::Deserialize)]
pub struct PricingQuoteRequest {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
}

pub async fn pricing_quote(
    State(state): State<AppState>,
    Json(request): Json<PricingQuoteRequest>,
) -> Result<Json<crate::handlers::pricing_types::PricingQuoteResponse>, ApiError> {
    let use_case = CalculatePricingQuoteUseCase::new(state.pricing_config_repository.clone());

    let input = PricingInput {
        monitors: request.monitors,
        interval_seconds: request.interval_seconds,
        retention_days: request.retention_days,
    };

    let quote = use_case
        .execute(input)
        .await
        .map_err(|e| match e {
            application::use_cases::calculate_pricing_quote::CalculatePricingQuoteError::NoPricingConfig => {
                ApiError::internal("pricing configuration missing")
            }
            application::use_cases::calculate_pricing_quote::CalculatePricingQuoteError::Validation(v) => {
                match v {
                    domain::pricing::PricingValidationError::MonitorsZero => {
                        ApiError::bad_request("monitors must be greater than zero")
                    }
                    domain::pricing::PricingValidationError::IntervalTooLow { min } => {
                        ApiError::bad_request(format!("interval_seconds must be at least {min} seconds"))
                    }
                    domain::pricing::PricingValidationError::MonitorsExceedMax { max } => {
                        ApiError::bad_request(format!("monitors exceeds maximum allowed ({max})"))
                    }
                    domain::pricing::PricingValidationError::RetentionExceedsMax { max } => {
                        ApiError::bad_request(format!("retention_days exceeds maximum allowed ({max})"))
                    }
                }
            }
            application::use_cases::calculate_pricing_quote::CalculatePricingQuoteError::Other(e) => {
                ApiError::internal_with_source("pricing calculation failed", e)
            }
        })?;

    Ok(Json(quote.into()))
}


pub async fn pricing_quote_authenticated(
    State(state): State<AppState>,
    Extension(user): Extension<domain::shared::types::AuthenticatedUser>,
    Json(request): Json<PricingQuoteRequest>,
) -> Result<Json<crate::handlers::pricing_types::PricingQuoteResponse>, ApiError> {
    user.require(domain::shared::types::Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;
    pricing_quote(State(state), Json(request)).await
}

