use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use domain::Check;
use domain::shared::ids::{CheckId, MonitorId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait CheckRepository: Send + Sync {
    async fn save(&self, check: &Check) -> RepoResult<()>;

    async fn find_by_id(&self, id: CheckId) -> RepoResult<Option<Check>>;

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Check>>;

    async fn list_checks(&self, limit: u32, offset: u32) -> RepoResult<Vec<Check>>;

    async fn delete_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<u64>;
}

#[async_trait]
impl<T> CheckRepository for Arc<T>
where
    T: CheckRepository + ?Sized + Send + Sync,
{
    async fn save(&self, check: &Check) -> RepoResult<()> {
        (**self).save(check).await
    }

    async fn find_by_id(&self, id: CheckId) -> RepoResult<Option<Check>> {
        (**self).find_by_id(id).await
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Check>> {
        (**self).list_by_monitor(monitor_id).await
    }

    async fn list_checks(&self, limit: u32, offset: u32) -> RepoResult<Vec<Check>> {
        (**self).list_checks(limit, offset).await
    }

    async fn delete_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<u64> {
        (**self).delete_by_monitor(monitor_id).await
    }
}
