use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use domain::shared::ids::TenantId;

use crate::dto::DashboardOverview;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Abstraction for fetching aggregated dashboard data.
#[async_trait]
pub trait DashboardRepository: Send + Sync {
    async fn get_overview(&self, tenant_id: TenantId) -> RepoResult<DashboardOverview>;
}

#[async_trait]
impl<T> DashboardRepository for Arc<T>
where
    T: DashboardRepository + ?Sized + Send + Sync,
{
    async fn get_overview(&self, tenant_id: TenantId) -> RepoResult<DashboardOverview> {
        (**self).get_overview(tenant_id).await
    }
}
