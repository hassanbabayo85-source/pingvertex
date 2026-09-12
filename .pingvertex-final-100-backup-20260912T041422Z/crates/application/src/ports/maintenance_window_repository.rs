use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::error::Error;
use std::sync::Arc;

use domain::MaintenanceWindow;
use domain::shared::ids::{MaintenanceWindowId, MonitorId, TenantId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait MaintenanceWindowRepository: Send + Sync {
    async fn create(&self, window: &MaintenanceWindow) -> RepoResult<()>;

    /// Lists all maintenance windows that are active at `now` for `monitor_id`.
    /// No user check — worker-only.
    async fn list_active_by_monitor(
        &self,
        monitor_id: MonitorId,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<MaintenanceWindow>>;

    /// SECURITY (hardening): deletes only when the window belongs to
    /// `tenant_id`, enforced at the SQL layer. There is no HTTP endpoint
    /// wired to maintenance-window deletion yet; keeping the repository
    /// operation tenant-scoped prevents a future endpoint from gaining an
    /// unscoped delete primitive by accident.
    #[allow(unused_variables)]
    async fn delete_for_tenant(
        &self,
        id: MaintenanceWindowId,
        tenant_id: TenantId,
    ) -> RepoResult<bool> {
        unimplemented!()
    }

    /// Lists every maintenance window configured for `monitor_id`, with
    /// no ownership check. Safe to use once the caller has already
    /// verified `monitor_id` belongs to their tenant (e.g. via
    /// `MonitorRepository::find_by_id` + `AuthorizationPolicy::same_tenant`),
    /// which makes a further per-row ownership check redundant. Unlike
    /// `list_by_monitor_for_user`, this returns a plain `Vec` (not
    /// `Option<Vec<_>>`) — the monitor-existence check happens before
    /// this is called.
    #[allow(unused_variables)]
    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<MaintenanceWindow>> {
        unimplemented!()
    }
}

#[async_trait]
impl<T> MaintenanceWindowRepository for Arc<T>
where
    T: MaintenanceWindowRepository + Send + Sync + ?Sized,
{
    async fn create(&self, window: &MaintenanceWindow) -> RepoResult<()> {
        (**self).create(window).await
    }

    async fn list_active_by_monitor(
        &self,
        monitor_id: MonitorId,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<MaintenanceWindow>> {
        (**self).list_active_by_monitor(monitor_id, now).await
    }

    async fn delete_for_tenant(
        &self,
        id: MaintenanceWindowId,
        tenant_id: TenantId,
    ) -> RepoResult<bool> {
        (**self).delete_for_tenant(id, tenant_id).await
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<MaintenanceWindow>> {
        (**self).list_by_monitor(monitor_id).await
    }
}
