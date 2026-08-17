use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::error::Error;
use std::sync::Arc;

use domain::MaintenanceWindow;
use domain::shared::ids::{MaintenanceWindowId, MonitorId, UserId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait MaintenanceWindowRepository: Send + Sync {
    async fn create(&self, window: &MaintenanceWindow) -> RepoResult<()>;

    async fn delete(&self, id: MaintenanceWindowId) -> RepoResult<bool>;

    async fn find_by_id_for_user(
        &self,
        id: MaintenanceWindowId,
        user_id: UserId,
    ) -> RepoResult<Option<MaintenanceWindow>>;

    async fn list_by_monitor_for_user(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<Vec<MaintenanceWindow>>>;

    /// Lists all maintenance windows that are active at `now` for `monitor_id`.
    /// No user check — worker-only.
    async fn list_active_by_monitor(
        &self,
        monitor_id: MonitorId,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<MaintenanceWindow>>;
}

#[async_trait]
impl<T> MaintenanceWindowRepository for Arc<T>
where
    T: MaintenanceWindowRepository + Send + Sync + ?Sized,
{
    async fn create(&self, window: &MaintenanceWindow) -> RepoResult<()> {
        (**self).create(window).await
    }

    async fn delete(&self, id: MaintenanceWindowId) -> RepoResult<bool> {
        (**self).delete(id).await
    }

    async fn find_by_id_for_user(
        &self,
        id: MaintenanceWindowId,
        user_id: UserId,
    ) -> RepoResult<Option<MaintenanceWindow>> {
        (**self).find_by_id_for_user(id, user_id).await
    }

    async fn list_by_monitor_for_user(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<Vec<MaintenanceWindow>>> {
        (**self).list_by_monitor_for_user(monitor_id, user_id).await
    }

    async fn list_active_by_monitor(
        &self,
        monitor_id: MonitorId,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<MaintenanceWindow>> {
        (**self).list_active_by_monitor(monitor_id, now).await
    }
}
