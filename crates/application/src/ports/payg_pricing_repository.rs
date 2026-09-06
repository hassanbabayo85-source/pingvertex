use async_trait::async_trait;
use domain::PAYGPricing;
use std::error::Error;
use std::sync::Arc;

pub type PaygPricingRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait PAYGPricingRepository: Send + Sync {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>>;
    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>>;
}

#[async_trait]
impl<T> PAYGPricingRepository for Arc<T>
where
    T: PAYGPricingRepository + Send + Sync + ?Sized,
{
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        (**self).find_active(operation).await
    }
    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> { (**self).list_active().await }
}

