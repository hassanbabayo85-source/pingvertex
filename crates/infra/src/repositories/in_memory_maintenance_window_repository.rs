use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, RwLock},
};

use application::ports::MaintenanceWindowRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::MaintenanceWindow;
use domain::shared::ids::{MaintenanceWindowId, MonitorId};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Simple in-memory maintenance window repository for development and
/// tests — mirrors `PostgresMaintenanceWindowRepository`.
///
#[derive(Debug, Clone, Default)]
pub struct InMemoryMaintenanceWindowRepository {
    windows: Arc<RwLock<HashMap<MaintenanceWindowId, MaintenanceWindow>>>,
}

impl InMemoryMaintenanceWindowRepository {
    pub fn new() -> Self {
        Self {
            windows: Arc::new(RwLock::new(HashMap::new())),
        }
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
