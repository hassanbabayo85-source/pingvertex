use crate::ports::PricingConfigRepository;
use domain::pricing::{PricingInput, PricingQuote, calculate_price};
use std::{error::Error, sync::Arc};

#[derive(Debug)]
pub enum CalculatePricingQuoteError {
    NoPricingConfig,
    Validation(domain::pricing::PricingValidationError),
    Other(Box<dyn Error + Send + Sync>),
}

impl std::fmt::Display for CalculatePricingQuoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPricingConfig => write!(f, "no active pricing configuration found"),
            Self::Validation(e) => write!(f, "pricing validation error: {e}"),
            Self::Other(e) => write!(f, "internal error: {e}"),
        }
    }
}

impl Error for CalculatePricingQuoteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NoPricingConfig => None,
            Self::Validation(_) => None,
            Self::Other(e) => Some(e.as_ref()),
        }
    }
}

pub struct CalculatePricingQuoteUseCase {
    pricing_config_repository: Arc<dyn PricingConfigRepository>,
}

impl CalculatePricingQuoteUseCase {
    pub fn new(pricing_config_repository: Arc<dyn PricingConfigRepository>) -> Self {
        Self {
            pricing_config_repository,
        }
    }

    pub async fn execute(
        &self,
        input: PricingInput,
    ) -> Result<PricingQuote, CalculatePricingQuoteError> {
        let config = self
            .pricing_config_repository
            .find_active_config()
            .await
            .map_err(CalculatePricingQuoteError::Other)?
            .ok_or(CalculatePricingQuoteError::NoPricingConfig)?;

        calculate_price(input, &config).map_err(CalculatePricingQuoteError::Validation)
    }
}
