use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use application::ports::MonitorRepository;
use application::ports::monitor_repository::QuotaCheckOutcome;
use async_trait::async_trait;
use domain::Monitor;
use domain::shared::ids::{MonitorId, TenantId, UserId};

/// Simple in-memory repository for development and tests.
#[derive(Debug, Clone, Default)]
pub struct InMemoryMonitorRepository {
    monitors: Arc<RwLock<HashMap<MonitorId, Monitor>>>,
}

impl InMemoryMonitorRepository {
    pub fn new() -> Self {
        Self {
            monitors: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl MonitorRepository for InMemoryMonitorRepository {
    async fn save(
        &self,
        monitor: &Monitor,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.monitors
            .write()
            .unwrap()
            .insert(monitor.id, monitor.clone());

        Ok(())
    }

    async fn find_by_id(
        &self,
        id: MonitorId,
    ) -> Result<Option<Monitor>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.monitors.read().unwrap().get(&id).cloned())
    }

    async fn find_all(&self) -> Result<Vec<Monitor>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.monitors.read().unwrap().values().cloned().collect())
    }

    async fn find_by_id_for_user(
        &self,
        id: MonitorId,
        user_id: UserId,
    ) -> Result<Option<Monitor>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .monitors
            .read()
            .unwrap()
            .get(&id)
            .filter(|monitor| monitor.user_id == user_id)
            .cloned())
    }

    async fn find_all_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<Monitor>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .monitors
            .read()
            .unwrap()
            .values()
            .filter(|monitor| monitor.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn find_all_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<Monitor>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .monitors
            .read()
            .unwrap()
            .values()
            .filter(|monitor| monitor.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn delete_for_user(
        &self,
        id: MonitorId,
        user_id: UserId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let mut monitors = self.monitors.write().unwrap();

        let belongs_to_user = monitors
            .get(&id)
            .map(|monitor| monitor.user_id == user_id)
            .unwrap_or(false);

        if belongs_to_user {
            Ok(monitors.remove(&id).is_some())
        } else {
            Ok(false)
        }
    }

    async fn update(
        &self,
        monitor: &Monitor,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.monitors
            .write()
            .unwrap()
            .insert(monitor.id, monitor.clone());

        Ok(())
    }

    async fn delete(
        &self,
        id: MonitorId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.monitors.write().unwrap().remove(&id).is_some())
    }

    async fn delete_for_tenant(
        &self,
        id: MonitorId,
        tenant_id: TenantId,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let mut monitors = self.monitors.write().unwrap();

        let belongs_to_tenant = monitors
            .get(&id)
            .map(|monitor| monitor.tenant_id == tenant_id)
            .unwrap_or(false);

        if belongs_to_tenant {
            Ok(monitors.remove(&id).is_some())
        } else {
            Ok(false)
        }
    }

    async fn find_by_heartbeat_token(
        &self,
        token: &str,
    ) -> Result<Option<Monitor>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .monitors
            .read()
            .unwrap()
            .values()
            .find(|monitor| {
                monitor
                    .heartbeat_token
                    .as_ref()
                    .map(|t| t.as_str() == token)
                    .unwrap_or(false)
            })
            .cloned())
    }

    async fn update_last_heartbeat_at(
        &self,
        id: MonitorId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(monitor) = self.monitors.write().unwrap().get_mut(&id) {
            monitor.last_heartbeat_at = Some(now);
            monitor.updated_at = now;
        }
        Ok(())
    }

    /// In-process equivalent of the Postgres advisory-lock pattern: a
    /// single `write()` lock guard is held across BOTH the count and the
    /// insert, so no other `save`/`save_if_within_quota` call (which all
    /// go through the same `RwLock`) can interleave between them.
    async fn save_if_within_quota(
        &self,
        monitor: &Monitor,
        tenant_id: TenantId,
        limit: u32,
    ) -> Result<QuotaCheckOutcome, Box<dyn std::error::Error + Send + Sync>> {
        let mut monitors = self.monitors.write().unwrap();

        let current_count = monitors
            .values()
            .filter(|existing| existing.tenant_id == tenant_id)
            .count() as u32;

        if current_count >= limit {
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }

        monitors.insert(monitor.id, monitor.clone());
        Ok(QuotaCheckOutcome::Saved)
    }
}
