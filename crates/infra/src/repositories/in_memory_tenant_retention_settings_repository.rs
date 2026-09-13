use application::ports::{
    TenantRetentionRepoResult as RepoResult, TenantRetentionSettingsRepository,
};
use async_trait::async_trait;
use domain::entities::TenantRetentionSettings;
use domain::shared::ids::TenantId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct InMemoryTenantRetentionSettingsRepository {
    inner: Arc<Mutex<HashMap<TenantId, TenantRetentionSettings>>>,
}

impl InMemoryTenantRetentionSettingsRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TenantRetentionSettingsRepository for InMemoryTenantRetentionSettingsRepository {
    async fn get(&self, tenant_id: TenantId) -> RepoResult<Option<TenantRetentionSettings>> {
        let guard = self.inner.lock().unwrap();
        Ok(guard.get(&tenant_id).cloned())
    }

    async fn upsert(&self, settings: &TenantRetentionSettings) -> RepoResult<()> {
        let mut guard = self.inner.lock().unwrap();
        let should_write = match guard.get(&settings.tenant_id) {
            Some(existing) => settings.source.priority() > existing.source.priority(),
            None => true,
        };
        if should_write {
            guard.insert(settings.tenant_id, settings.clone());
        }
        Ok(())
    }

    async fn effective_retention(
        &self,
        tenant_id: TenantId,
        default_retention_days: u32,
        max_retention_days: u32,
    ) -> RepoResult<u32> {
        let guard = self.inner.lock().unwrap();
        let value = guard
            .get(&tenant_id)
            .filter(|s| s.retention_days > 0)
            .map(|s| s.retention_days)
            .unwrap_or(default_retention_days);
        Ok(value.min(max_retention_days).max(1))
    }
}
