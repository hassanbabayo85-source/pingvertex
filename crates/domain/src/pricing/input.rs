use crate::pricing::PricingConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PricingInput {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
}

impl PricingInput {
    pub fn validate(&self, config: &PricingConfig) -> Result<(), PricingValidationError> {
        if self.monitors == 0 {
            return Err(PricingValidationError::MonitorsZero);
        }
        if self.interval_seconds < config.min_interval_seconds {
            return Err(PricingValidationError::IntervalTooLow {
                min: config.min_interval_seconds,
            });
        }
        if self.monitors > config.max_monitors {
            return Err(PricingValidationError::MonitorsExceedMax {
                max: config.max_monitors,
            });
        }
        if self.retention_days > config.max_retention_days {
            return Err(PricingValidationError::RetentionExceedsMax {
                max: config.max_retention_days,
            });
        }
        Ok(())
    }

    pub fn matches_free_tier(&self, config: &PricingConfig) -> bool {
        self.monitors <= config.free_tier.monitors
            && self.interval_seconds >= config.free_tier.interval_seconds
            && self.retention_days <= config.free_tier.retention_days
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum PricingValidationError {
    #[error("monitors must be greater than zero")]
    MonitorsZero,
    #[error("interval_seconds must be at least {min} seconds")]
    IntervalTooLow { min: u32 },
    #[error("monitors exceeds maximum allowed ({max})")]
    MonitorsExceedMax { max: u64 },
    #[error("retention_days exceeds maximum allowed ({max})")]
    RetentionExceedsMax { max: u32 },
}
