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

    /// Deletes a maintenance window, but only if it belongs (via its
    /// monitor) to `user_id`. Mirrors the `delete_for_user` pattern used
    /// everywhere else in this codebase (see
    /// `NotificationChannelRepository::delete_for_user`,
    /// `MonitorRepository::delete`) — ownership is enforced in the SQL
    /// `WHERE` clause itself, not left to callers to check first, so a
    /// future handler built on top of this can't accidentally introduce
    /// an IDOR by forgetting an ownership check. Returns `false` (not an
    /// error) both when the window doesn't exist AND when it exists but
    /// belongs to someone else — the two cases are indistinguishable to
    /// the caller on purpose, so a 404 never confirms "that id belongs to
    /// another user".
    async fn delete_for_user(&self, id: MaintenanceWindowId, user_id: UserId) -> RepoResult<bool>;

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

    async fn delete_for_user(&self, id: MaintenanceWindowId, user_id: UserId) -> RepoResult<bool> {
        (**self).delete_for_user(id, user_id).await
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
