use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::{Arc, Mutex};

use application::ports::HistoryRetentionRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::Check;
use domain::shared::ids::{CheckId, MonitorId, TenantId};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Test-only in-memory implementation for retention deletion.
#[derive(Debug, Clone, Default)]
pub struct InMemoryHistoryRetentionRepository {
    checks: Arc<Mutex<Vec<Check>>>,
    monitor_tenants: Arc<Mutex<HashMap<MonitorId, TenantId>>>,
}

impl InMemoryHistoryRetentionRepository {
    pub fn new() -> Self {
        Self {
            checks: Arc::new(Mutex::new(Vec::new())),
            monitor_tenants: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Seeds a check and associates its `monitor_id` with the given
    /// tenant. Called directly by tests, not part of the port.
    pub fn seed_check(&self, check: Check, tenant_id: TenantId) {
        self.monitor_tenants
            .lock()
            .unwrap()
            .insert(check.monitor_id, tenant_id);
        self.checks.lock().unwrap().push(check);
    }

    pub fn list_check_ids(&self) -> Vec<CheckId> {
        self.checks
            .lock()
            .unwrap()
            .iter()
            .map(|check| check.id)
            .collect()
    }
}

#[async_trait]
impl HistoryRetentionRepository for InMemoryHistoryRetentionRepository {
    async fn delete_older_than_batch(
        &self,
        tenant_id: TenantId,
        cutoff: DateTime<Utc>,
        batch_size: u32,
    ) -> RepoResult<u64> {
        let mut checks = self.checks.lock().unwrap();
        let monitor_tenants = self.monitor_tenants.lock().unwrap();

        let mut to_delete = HashSet::new();
        for check in checks.iter() {
            if to_delete.len() as u32 == batch_size {
                break;
            }
            if monitor_tenants.get(&check.monitor_id).copied() == Some(tenant_id)
                && check.checked_at < cutoff
            {
                to_delete.insert(check.id);
            }
        }

        if to_delete.is_empty() {
            return Ok(0);
        }

        let before = checks.len();
        checks.retain(|check| !to_delete.contains(&check.id));
        let deleted = before - checks.len();
        Ok(deleted as u64)
    }
}
