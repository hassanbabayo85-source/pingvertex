use async_trait::async_trait;
use domain::PricingAuditLogEntry;
use domain::pricing::PricingConfig;
use std::{error::Error, sync::Arc};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait PricingConfigRepository: Send + Sync {
    async fn find_active_config(&self) -> RepoResult<Option<PricingConfig>>;
    async fn find_by_version(&self, version: u32) -> RepoResult<Option<PricingConfig>>;
    /// Creates a new version of pricing config.
    /// `reason` is used for audit log entry.
    async fn create_version(
        &self,
        config: &PricingConfig,
        reason: Option<String>,
    ) -> RepoResult<()>;
    /// Records an audit log entry for pricing config changes.
    async fn record_audit_log(&self, entry: &PricingAuditLogEntry) -> RepoResult<()>;
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

    async fn create_version(
        &self,
        config: &PricingConfig,
        reason: Option<String>,
    ) -> RepoResult<()> {
        (**self).create_version(config, reason).await
    }

    async fn record_audit_log(&self, entry: &PricingAuditLogEntry) -> RepoResult<()> {
        (**self).record_audit_log(entry).await
    }

    async fn list_versions(&self) -> RepoResult<Vec<PricingConfig>> {
        (**self).list_versions().await
    }
}
