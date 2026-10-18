use std::{error::Error, fmt, sync::Arc};

use chrono::{DateTime, Utc};
use domain::pricing::calculate_period_cost;
use domain::shared::ids::SubscriptionId;
use rust_decimal::Decimal;

use crate::ports::{PricingConfigRepository, SubscriptionUsagePeriodRepository};

#[derive(Debug)]
pub enum CalculateSubscriptionCycleCostError {
    NoPricingConfig,
    Validation(domain::pricing::PricingValidationError),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CalculateSubscriptionCycleCostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPricingConfig => write!(f, "no active pricing configuration found"),
            Self::Validation(e) => write!(f, "pricing validation error: {e}"),
            Self::Other(e) => write!(f, "internal error: {e}"),
        }
    }
}

impl Error for CalculateSubscriptionCycleCostError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NoPricingConfig => None,
            Self::Validation(_) => None,
            Self::Other(e) => Some(e.as_ref()),
        }
    }
}

/// Proration Service: billing-time integration point.
///
/// Sums `calculate_period_cost` across every `SubscriptionUsagePeriod`
/// that touched `[cycle_start, cycle_end)`, instead of charging a single
/// flat rate for whatever usage happens to be current when billing runs.
/// Intended to be called by whatever future code builds the actual
/// invoice-generation flow — no such flow exists yet in this codebase
/// (out of scope here).
pub struct CalculateSubscriptionCycleCostUseCase {
    usage_period_repository: Arc<dyn SubscriptionUsagePeriodRepository>,
    pricing_config_repository: Arc<dyn PricingConfigRepository>,
}

impl CalculateSubscriptionCycleCostUseCase {
    pub fn new(
        usage_period_repository: Arc<dyn SubscriptionUsagePeriodRepository>,
        pricing_config_repository: Arc<dyn PricingConfigRepository>,
    ) -> Self {
        Self {
            usage_period_repository,
            pricing_config_repository,
        }
    }

    pub async fn execute(
        &self,
        subscription_id: SubscriptionId,
        cycle_start: DateTime<Utc>,
        cycle_end: DateTime<Utc>,
    ) -> Result<Decimal, CalculateSubscriptionCycleCostError> {
        let config = self
            .pricing_config_repository
            .find_active_config()
            .await
            .map_err(CalculateSubscriptionCycleCostError::Other)?
            .ok_or(CalculateSubscriptionCycleCostError::NoPricingConfig)?;

        let periods = self
            .usage_period_repository
            .list_periods_for_cycle(subscription_id, cycle_start, cycle_end)
            .await
            .map_err(CalculateSubscriptionCycleCostError::Other)?;

        let mut total = Decimal::ZERO;
        for period in &periods {
            total += calculate_period_cost(period, &config, cycle_start, cycle_end)
                .map_err(CalculateSubscriptionCycleCostError::Validation)?;
        }

        Ok(total)
    }
}
