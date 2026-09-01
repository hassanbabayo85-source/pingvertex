use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, RwLock},
};

use application::ports::MaintenanceWindowRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::MaintenanceWindow;
use domain::shared::ids::{MaintenanceWindowId, MonitorId, TenantId};

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

        windows.sort_by_key(|b| std::cmp::Reverse(b.starts_at));
        Ok(windows)
    }

    async fn delete_for_tenant(
        &self,
        id: MaintenanceWindowId,
        tenant_id: TenantId,
    ) -> RepoResult<bool> {
        let mut windows = self.windows.write().unwrap();

        let belongs_to_tenant = windows
            .get(&id)
            .map(|window| window.tenant_id == tenant_id)
            .unwrap_or(false);

        if belongs_to_tenant {
            Ok(windows.remove(&id).is_some())
        } else {
            Ok(false)
        }
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

        windows.sort_by_key(|b| std::cmp::Reverse(b.starts_at));
        Ok(windows)
    }
}
