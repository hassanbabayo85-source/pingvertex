use async_trait::async_trait;
use domain::entities::TenantRetentionSettings;
use domain::shared::ids::TenantId;
use std::{error::Error, sync::Arc};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence for `TenantRetentionSettings`.
///
/// `effective_retention` is the method callers should use in almost every
/// case — it resolves the tenant's row (if any) against the supplied
/// pricing-config bounds and returns a value safe to apply to a
/// `DELETE ... WHERE checked_at < cutoff`.
#[async_trait]
pub trait TenantRetentionSettingsRepository: Send + Sync {
    async fn get(&self, tenant_id: TenantId) -> RepoResult<Option<TenantRetentionSettings>>;

    /// Insert or update the tenant's retention. If a row already exists
    /// with a higher-priority source (e.g. an `Addon` row exists and this
    /// call is for a `Default`), the existing value is preserved.
    async fn upsert(&self, settings: &TenantRetentionSettings) -> RepoResult<()>;

    /// The retention the retention worker should apply for `tenant_id`.
    ///
    ///   * If the tenant has a row whose `retention_days` is within
    ///     `[1, max_retention_days]`, that value is returned.
    ///   * Otherwise `default_retention_days` is returned.
    ///   * The result is clamped to `max_retention_days`.
    async fn effective_retention(
        &self,
        tenant_id: TenantId,
        default_retention_days: u32,
        max_retention_days: u32,
    ) -> RepoResult<u32>;
}

#[async_trait]
impl<T> TenantRetentionSettingsRepository for Arc<T>
where
    T: TenantRetentionSettingsRepository + Send + Sync + ?Sized,
{
    async fn get(&self, tenant_id: TenantId) -> RepoResult<Option<TenantRetentionSettings>> {
        (**self).get(tenant_id).await
    }

    async fn upsert(&self, settings: &TenantRetentionSettings) -> RepoResult<()> {
        (**self).upsert(settings).await
    }

    async fn effective_retention(
        &self,
        tenant_id: TenantId,
        default_retention_days: u32,
        max_retention_days: u32,
    ) -> RepoResult<u32> {
        (**self)
            .effective_retention(tenant_id, default_retention_days, max_retention_days)
            .await
    }
}
