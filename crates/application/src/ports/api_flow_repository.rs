use async_trait::async_trait;
use std::sync::Arc;

use domain::entities::ApiFlow;
use domain::shared::ids::{ApiFlowId, MonitorId};

use crate::ports::monitor_repository::RepoResult;

#[async_trait]
pub trait ApiFlowRepository: Send + Sync {
    async fn save(&self, flow: &ApiFlow) -> RepoResult<()>;

    async fn update(&self, flow: &ApiFlow) -> RepoResult<()>;

    async fn delete(&self, id: ApiFlowId) -> RepoResult<bool>;

    /// SECURITY (P0): tenant-scoped delete — deletes only if the
    /// flow belongs to `tenant_id`. Prefer this over `delete` for
    /// anything reachable from an authenticated HTTP request.
    async fn delete_for_tenant(
        &self,
        _id: ApiFlowId,
        _tenant_id: domain::shared::ids::TenantId,
    ) -> RepoResult<bool> {
        unimplemented!()
    }

    async fn find_by_id(&self, id: ApiFlowId) -> RepoResult<Option<ApiFlow>>;

    async fn find_by_monitor_id(&self, monitor_id: MonitorId) -> RepoResult<Option<ApiFlow>>;
}

#[async_trait]
impl<T> ApiFlowRepository for Arc<T>
where
    T: ApiFlowRepository + Send + Sync + ?Sized,
{
    async fn save(&self, flow: &ApiFlow) -> RepoResult<()> {
        (**self).save(flow).await
    }

    async fn update(&self, flow: &ApiFlow) -> RepoResult<()> {
        (**self).update(flow).await
    }

    async fn delete(&self, id: ApiFlowId) -> RepoResult<bool> {
        (**self).delete(id).await
    }

    async fn delete_for_tenant(
        &self,
        id: ApiFlowId,
        tenant_id: domain::shared::ids::TenantId,
    ) -> RepoResult<bool> {
        (**self).delete_for_tenant(id, tenant_id).await
    }

    async fn find_by_id(&self, id: ApiFlowId) -> RepoResult<Option<ApiFlow>> {
        (**self).find_by_id(id).await
    }

    async fn find_by_monitor_id(&self, monitor_id: MonitorId) -> RepoResult<Option<ApiFlow>> {
        (**self).find_by_monitor_id(monitor_id).await
    }
}
