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
    billing_blocks: Arc<RwLock<HashMap<MonitorId, (TenantId, String)>>>,
}

impl InMemoryMonitorRepository {
    pub fn new() -> Self {
        Self {
            monitors: Arc::new(RwLock::new(HashMap::new())),
            billing_blocks: Arc::new(RwLock::new(HashMap::new())),
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
            let removed = monitors.remove(&id).is_some();
            drop(monitors);
            if removed {
                self.billing_blocks.write().unwrap().remove(&id);
            }
            Ok(removed)
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
        let removed = self.monitors.write().unwrap().remove(&id).is_some();
        if removed {
            self.billing_blocks.write().unwrap().remove(&id);
        }
        Ok(removed)
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
            let removed = monitors.remove(&id).is_some();
            drop(monitors);
            if removed {
                self.billing_blocks.write().unwrap().remove(&id);
            }
            Ok(removed)
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

    async fn set_enabled_for_tenant(
        &self,
        tenant_id: TenantId,
        monitor_ids: Option<&[MonitorId]>,
        enabled: bool,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let ids = monitor_ids.map(|v| v.iter().copied().collect::<std::collections::HashSet<_>>());
        let mut monitors = self.monitors.write().unwrap();
        let mut changed = 0u64;
        for monitor in monitors.values_mut() {
            if monitor.tenant_id != tenant_id
                || ids.as_ref().is_some_and(|set| !set.contains(&monitor.id))
            {
                continue;
            }
            if monitor.enabled != enabled {
                monitor.enabled = enabled;
                monitor.updated_at = chrono::Utc::now();
                changed += 1;
            }
        }
        Ok(changed)
    }

    async fn set_billing_block(
        &self,
        monitor_id: MonitorId,
        tenant_id: TenantId,
        reason: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let belongs = self
            .monitors
            .read()
            .unwrap()
            .get(&monitor_id)
            .map(|m| m.tenant_id == tenant_id)
            .unwrap_or(false);
        if !belongs || reason.trim().is_empty() {
            return Ok(false);
        }
        self.billing_blocks
            .write()
            .unwrap()
            .insert(monitor_id, (tenant_id, reason.trim().to_owned()));
        Ok(true)
    }

    async fn clear_billing_blocks_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let mut blocks = self.billing_blocks.write().unwrap();
        let before = blocks.len();
        blocks.retain(|_, (tid, _)| *tid != tenant_id);
        Ok((before - blocks.len()) as u64)
    }

    async fn billing_blocks_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> Result<HashMap<MonitorId, String>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .billing_blocks
            .read()
            .unwrap()
            .iter()
            .filter(|(_, (tid, _))| *tid == tenant_id)
            .map(|(id, (_, reason))| (*id, reason.clone()))
            .collect())
    }
}
