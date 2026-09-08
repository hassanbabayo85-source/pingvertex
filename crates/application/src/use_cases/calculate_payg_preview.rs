use crate::ports::{MonitorIntervalPolicyRepository, PAYGPricingRepository};
use domain::MonitorKind;
use std::{error::Error, fmt, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaygPreviewInput {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub regions: u32,
    pub forecast_days: u32,
    pub kind: MonitorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaygPreview {
    pub checks_per_day: u64,
    pub checks_total: u64,
    pub monitors: u64,
    pub regions: u32,
    pub forecast_days: u32,
    pub interval_seconds: u32,
    pub unit_price: i64,
    pub currency: String,
    pub pricing_version: i64,
    pub estimated_cost_units: i64,
}

#[derive(Debug)]
pub enum PaygPreviewError {
    Invalid(&'static str),
    NoPricing,
    Overflow,
    Repository(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for PaygPreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "{message}"),
            Self::NoPricing => write!(f, "no active PAYG pricing found"),
            Self::Overflow => write!(f, "PAYG forecast exceeds supported numeric range"),
            Self::Repository(_) => write!(f, "failed to load PAYG pricing"),
        }
    }
}

impl Error for PaygPreviewError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

pub struct CalculatePaygPreviewUseCase {
    pricing_repository: Arc<dyn PAYGPricingRepository>,
    interval_policy_repository: Option<Arc<dyn MonitorIntervalPolicyRepository>>,
}

impl CalculatePaygPreviewUseCase {
    pub fn new(pricing_repository: Arc<dyn PAYGPricingRepository>) -> Self {
        Self {
            pricing_repository,
            interval_policy_repository: None,
        }
    }

    pub fn with_interval_policy_repository(
        mut self,
        repository: Arc<dyn MonitorIntervalPolicyRepository>,
    ) -> Self {
        self.interval_policy_repository = Some(repository);
        self
    }

    pub async fn execute(&self, input: PaygPreviewInput) -> Result<PaygPreview, PaygPreviewError> {
        if input.monitors == 0 {
            return Err(PaygPreviewError::Invalid(
                "monitors must be greater than zero",
            ));
        }
        if input.regions == 0 || input.regions > 32 {
            return Err(PaygPreviewError::Invalid(
                "regions must be between 1 and 32",
            ));
        }
        if input.forecast_days == 0 || input.forecast_days > 3650 {
            return Err(PaygPreviewError::Invalid(
                "forecast_days must be between 1 and 3650",
            ));
        }
        let min_interval = if let Some(policies) = &self.interval_policy_repository {
            policies
                .minimum_interval_seconds(input.kind)
                .await
                .map_err(PaygPreviewError::Repository)?
        } else {
            match input.kind {
                MonitorKind::Flow => 60,
                _ => 30,
            }
        };
        if input.interval_seconds < min_interval || input.interval_seconds > 86_400 {
            return Err(PaygPreviewError::Invalid(
                "interval_seconds is outside the allowed range for this monitor type",
            ));
        }

        let pricing_operation = domain::monitor_operation(input.kind, false);
        let pricing = self
            .pricing_repository
            .find_active(pricing_operation.as_str())
            .await
            .map_err(PaygPreviewError::Repository)?
            .ok_or(PaygPreviewError::NoPricing)?;

        if pricing.unit_price <= 0 {
            return Err(PaygPreviewError::Invalid(
                "active PAYG unit price must be positive",
            ));
        }

        // Forecasts intentionally use completed whole check intervals per
        // day. Actual billing remains execution-based and can differ from a
        // forecast because of outages, schedule changes, or disabled periods.
        let checks_per_day = 86_400u64 / u64::from(input.interval_seconds.max(1));
        if checks_per_day == 0 {
            return Err(PaygPreviewError::Invalid(
                "interval_seconds must be at most 86400",
            ));
        }

        let checks_total = input
            .monitors
            .checked_mul(checks_per_day)
            .and_then(|v| v.checked_mul(u64::from(input.regions)))
            .and_then(|v| v.checked_mul(u64::from(input.forecast_days)))
            .ok_or(PaygPreviewError::Overflow)?;

        let estimated_cost_units = i64::try_from(checks_total)
            .ok()
            .and_then(|v| v.checked_mul(pricing.unit_price))
            .ok_or(PaygPreviewError::Overflow)?;

        Ok(PaygPreview {
            checks_per_day,
            checks_total,
            monitors: input.monitors,
            regions: input.regions,
            forecast_days: input.forecast_days,
            interval_seconds: input.interval_seconds,
            unit_price: pricing.unit_price,
            currency: pricing.currency,
            pricing_version: pricing.version,
            estimated_cost_units,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::PaygPricingRepoResult;
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use domain::PAYGPricing;
    use uuid::Uuid;

    struct TestPricingRepository {
        pricing: Vec<PAYGPricing>,
    }

    impl TestPricingRepository {
        fn new(pricing: Vec<PAYGPricing>) -> Self {
            Self { pricing }
        }
    }

    #[async_trait]
    impl PAYGPricingRepository for TestPricingRepository {
        async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
            Ok(self
                .pricing
                .iter()
                .find(|pricing| {
                    pricing.operation == operation
                        && pricing.active
                        && pricing.effective_until.is_none()
                })
                .cloned())
        }

        async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
            Ok(self
                .pricing
                .iter()
                .filter(|pricing| pricing.active && pricing.effective_until.is_none())
                .cloned()
                .collect())
        }

        async fn create_version(
            &self,
            _operation: &str,
            _unit_name: &str,
            _unit_price: i64,
            _currency: &str,
            _effective_from: chrono::DateTime<chrono::Utc>,
        ) -> PaygPricingRepoResult<PAYGPricing> {
            Err("create_version is not used by preview tests".into())
        }
    }

    #[tokio::test]
    async fn five_minute_single_region_thirty_day_forecast_matches_formula() {
        let pricing = Arc::new(TestPricingRepository::new(vec![PAYGPricing {
            id: Uuid::new_v4(),
            version: 1,
            operation: "http_check.monitoring".into(),
            unit_name: "check".into(),
            unit_price: 2,
            currency: "USD".into(),
            effective_from: Utc::now() - Duration::minutes(1),
            effective_until: None,
            active: true,
        }]));
        let preview = CalculatePaygPreviewUseCase::new(pricing)
            .execute(PaygPreviewInput {
                monitors: 100,
                interval_seconds: 300,
                regions: 1,
                forecast_days: 30,
                kind: MonitorKind::Http,
            })
            .await
            .unwrap();

        assert_eq!(preview.checks_per_day, 288);
        assert_eq!(preview.checks_total, 864_000);
        assert_eq!(preview.estimated_cost_units, 1_728_000);
    }

    #[tokio::test]
    async fn flow_preview_rejects_interval_below_sixty_seconds() {
        let pricing = Arc::new(TestPricingRepository::new(vec![PAYGPricing {
            id: Uuid::new_v4(),
            version: 1,
            operation: "api_flow_check.monitoring".into(),
            unit_name: "request/check".into(),
            unit_price: 4,
            currency: "USD".into(),
            effective_from: Utc::now() - Duration::minutes(1),
            effective_until: None,
            active: true,
        }]));
        let result = CalculatePaygPreviewUseCase::new(pricing)
            .execute(PaygPreviewInput {
                monitors: 1,
                interval_seconds: 30,
                regions: 1,
                forecast_days: 7,
                kind: MonitorKind::Flow,
            })
            .await;
        assert!(matches!(result, Err(PaygPreviewError::Invalid(_))));
    }
}
