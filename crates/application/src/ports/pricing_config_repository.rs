use async_trait::async_trait;
use domain::pricing::PricingConfig;
use std::{error::Error, sync::Arc};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait PricingConfigRepository: Send + Sync {
    async fn find_active_config(&self) -> RepoResult<Option<PricingConfig>>;
    async fn find_by_version(&self, version: u32) -> RepoResult<Option<PricingConfig>>;
    async fn create_version(&self, config: &PricingConfig) -> RepoResult<()>;
    async fn list_versions(&self) -> RepoResult<Vec<PricingConfig>>;
}

#[async_trait]
impl<T> PricingConfigRepository for Arc<T>
where
    T: PricingConfigRepository + Send + Sync + ?Sized,
{
    async fn find_active_config(&self) -> RepoResult<Option<PricingConfig>> {
        (**self).find_active_config().await
    }

    async fn find_by_version(&self, version: u32) -> RepoResult<Option<PricingConfig>> {
        (**self).find_by_version(version).await
    }

    async fn create_version(&self, config: &PricingConfig) -> RepoResult<()> {
        (**self).create_version(config).await
    }

    async fn list_versions(&self) -> RepoResult<Vec<PricingConfig>> {
        (**self).list_versions().await
    }
}
