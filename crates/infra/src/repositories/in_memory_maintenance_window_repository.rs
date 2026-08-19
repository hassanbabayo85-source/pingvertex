use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, RwLock},
};

use application::ports::{MaintenanceWindowRepository, MonitorRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::MaintenanceWindow;
use domain::shared::ids::{MaintenanceWindowId, MonitorId, UserId};

use crate::repositories::InMemoryMonitorRepository;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Simple in-memory maintenance window repository for development and
/// tests — mirrors `PostgresMaintenanceWindowRepository`.
///
/// Holds a reference to an `InMemoryMonitorRepository` (rather than being
/// fully standalone), for the same reason as
/// `InMemoryNotificationChannelRepository`: `list_by_monitor_for_user`
/// needs to reproduce the real repository's "monitor doesn't exist /
/// isn't yours → `None`" vs "owned monitor with zero windows →
/// `Some(vec![])`" distinction. Tests should construct this with the SAME
/// `InMemoryMonitorRepository` instance used elsewhere in the test.
#[derive(Debug, Clone)]
pub struct InMemoryMaintenanceWindowRepository {
    windows: Arc<RwLock<HashMap<MaintenanceWindowId, MaintenanceWindow>>>,
    monitors: InMemoryMonitorRepository,
}

impl InMemoryMaintenanceWindowRepository {
    pub fn new(monitors: InMemoryMonitorRepository) -> Self {
        Self {
            windows: Arc::new(RwLock::new(HashMap::new())),
            monitors,
        }
    }
}

impl Default for InMemoryMaintenanceWindowRepository {
    fn default() -> Self {
        Self::new(InMemoryMonitorRepository::new())
    }
}

#[async_trait]
impl MaintenanceWindowRepository for InMemoryMaintenanceWindowRepository {
    async fn create(&self, window: &MaintenanceWindow) -> RepoResult<()> {
        self.windows
            .write()
            .unwrap()
            .insert(window.id, window.clone());
        Ok(())
    }

    async fn delete_for_user(&self, id: MaintenanceWindowId, user_id: UserId) -> RepoResult<bool> {
        let monitor_id = self.windows.read().unwrap().get(&id).map(|w| w.monitor_id);

        let Some(monitor_id) = monitor_id else {
            return Ok(false);
        };

        let owns_monitor = self
            .monitors
            .find_by_id_for_user(monitor_id, user_id)
            .await?
            .is_some();

        if !owns_monitor {
            return Ok(false);
        }

        Ok(self.windows.write().unwrap().remove(&id).is_some())
    }

    async fn find_by_id_for_user(
        &self,
        id: MaintenanceWindowId,
        user_id: UserId,
    ) -> RepoResult<Option<MaintenanceWindow>> {
        let window = self.windows.read().unwrap().get(&id).cloned();

        let Some(window) = window else {
            return Ok(None);
        };

        let owns_monitor = self
            .monitors
            .find_by_id_for_user(window.monitor_id, user_id)
            .await?
            .is_some();

        Ok(if owns_monitor { Some(window) } else { None })
    }

    async fn list_by_monitor_for_user(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<Vec<MaintenanceWindow>>> {
        let owns_monitor = self
            .monitors
            .find_by_id_for_user(monitor_id, user_id)
            .await?
            .is_some();

        if !owns_monitor {
            return Ok(None);
        }

        let mut windows: Vec<MaintenanceWindow> = self
            .windows
            .read()
            .unwrap()
            .values()
            .filter(|w| w.monitor_id == monitor_id)
            .cloned()
            .collect();

        windows.sort_by(|a, b| b.starts_at.cmp(&a.starts_at));
        Ok(Some(windows))
    }

    async fn list_active_by_monitor(
        &self,
        monitor_id: MonitorId,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<MaintenanceWindow>> {
        let mut windows: Vec<MaintenanceWindow> = self
            .windows
            .read()
            .unwrap()
            .values()
            .filter(|w| w.monitor_id == monitor_id && w.starts_at <= now && w.ends_at >= now)
            .cloned()
            .collect();

        windows.sort_by(|a, b| b.starts_at.cmp(&a.starts_at));
        Ok(windows)
    }

    async fn find_by_id(&self, id: MaintenanceWindowId) -> RepoResult<Option<MaintenanceWindow>> {
        Ok(self.windows.read().unwrap().get(&id).cloned())
    }

    async fn delete(&self, id: MaintenanceWindowId) -> RepoResult<bool> {
        Ok(self.windows.write().unwrap().remove(&id).is_some())
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<MaintenanceWindow>> {
        let mut windows: Vec<MaintenanceWindow> = self
            .windows
            .read()
            .unwrap()
            .values()
            .filter(|w| w.monitor_id == monitor_id)
            .cloned()
            .collect();

        windows.sort_by(|a, b| b.starts_at.cmp(&a.starts_at));
        Ok(windows)
    }
}
