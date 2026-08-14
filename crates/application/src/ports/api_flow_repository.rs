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

    async fn find_by_id(&self, id: ApiFlowId) -> RepoResult<Option<ApiFlow>> {
        (**self).find_by_id(id).await
    }

    async fn find_by_monitor_id(&self, monitor_id: MonitorId) -> RepoResult<Option<ApiFlow>> {
        (**self).find_by_monitor_id(monitor_id).await
    }
}
