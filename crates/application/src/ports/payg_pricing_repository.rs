use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PAYGPricing;
use std::error::Error;
use std::sync::Arc;

pub type PaygPricingRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait PAYGPricingRepository: Send + Sync {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>>;

    /// Resolves the most specific active price for the trusted server-side context.
    /// Implementations should prefer exact context matches and fall back to the
    /// generic operation price when no contextual override exists.
    async fn find_active_for_context(
        &self,
        operation: &str,
        context: &PricingContext,
    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let _ = context;
        self.find_active(operation).await
    }

    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>>;

    /// Appends a new immutable PAYG price version. The repository owns the
    /// global version allocation so concurrent administrators cannot collide.
    async fn create_version(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
    ) -> PaygPricingRepoResult<PAYGPricing>;

    async fn create_version_with_context(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
        context: &PricingRuleContext,
    ) -> PaygPricingRepoResult<PAYGPricing> {
        let _ = context;
        self.create_version(operation, unit_name, unit_price, currency, effective_from)
            .await
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PricingContext {
    pub account_type: Option<domain::AccountType>,
    pub region: Option<domain::RegionCode>,
    pub monitor_kind: Option<domain::MonitorKind>,
    pub quantity: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PricingRuleContext {
    pub account_type: Option<domain::AccountType>,
    pub region: Option<domain::RegionCode>,
    pub monitor_kind: Option<domain::MonitorKind>,
    pub min_quantity: Option<u64>,
    pub max_quantity: Option<u64>,
}

pub async fn resolve_price(
    repository: &dyn PAYGPricingRepository,
    operation: &str,
    context: &PricingContext,
) -> PaygPricingRepoResult<Option<PAYGPricing>> {
    if operation.trim().is_empty() {
        return Ok(None);
    }
    repository.find_active_for_context(operation, context).await
}

#[async_trait]
impl<T> PAYGPricingRepository for Arc<T>
where
    T: PAYGPricingRepository + Send + Sync + ?Sized,
{
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        (**self).find_active(operation).await
    }
    async fn find_active_for_context(
        &self,
        operation: &str,
        context: &PricingContext,
    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        (**self).find_active_for_context(operation, context).await
    }
    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
        (**self).list_active().await
    }
    async fn create_version(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
    ) -> PaygPricingRepoResult<PAYGPricing> {
        (**self)
            .create_version(operation, unit_name, unit_price, currency, effective_from)
            .await
    }
    async fn create_version_with_context(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
        context: &PricingRuleContext,
    ) -> PaygPricingRepoResult<PAYGPricing> {
        (**self)
            .create_version_with_context(
                operation,
                unit_name,
                unit_price,
                currency,
                effective_from,
                context,
            )
            .await
    }
}
