use application::ports::PricingConfigRepository;
use async_trait::async_trait;
use domain::pricing::PricingConfig;
use domain::{PricingAuditLogEntry, shared::ids::AuditLogEntryId};
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Default)]
pub struct InMemoryPricingConfigRepository {
    configs: Arc<Mutex<HashMap<u32, PricingConfig>>>,
    audit_logs: Arc<Mutex<Vec<PricingAuditLogEntry>>>,
}

impl InMemoryPricingConfigRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(self, config: PricingConfig) -> Self {
        let mut map = self.configs.lock().unwrap();
        map.insert(config.version, config);
        drop(map);
        self
    }

    /// Returns all stored audit log entries (for testing).
    pub fn audit_logs(&self) -> Vec<PricingAuditLogEntry> {
        self.audit_logs.lock().unwrap().clone()
    }
}

#[async_trait]
impl PricingConfigRepository for InMemoryPricingConfigRepository {
    async fn find_active_config(
        &self,
    ) -> Result<Option<PricingConfig>, Box<dyn Error + Send + Sync>> {
        let configs = self.configs.lock().unwrap();
        Ok(configs.values().max_by_key(|c| c.version).cloned())
    }

    async fn find_by_version(
        &self,
        version: u32,
    ) -> Result<Option<PricingConfig>, Box<dyn Error + Send + Sync>> {
        Ok(self.configs.lock().unwrap().get(&version).cloned())
    }

    async fn create_version(
        &self,
        config: &PricingConfig,
        reason: Option<String>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        {
            let mut configs = self.configs.lock().unwrap();
            configs.insert(config.version, config.clone());
        }

        let prev_version = self
            .configs
            .lock()
            .unwrap()
            .keys()
            .filter(|&&v| v < config.version)
            .max()
            .cloned();

        let entry = PricingAuditLogEntry::new(
            AuditLogEntryId::new(),
            config.version,
            "version".to_string(),
            prev_version.map(|v| v.to_string()),
            Some(config.version.to_string()),
            config.created_by,
            reason,
        );
        self.audit_logs.lock().unwrap().push(entry);

        Ok(())
    }

    async fn record_audit_log(
        &self,
        entry: &PricingAuditLogEntry,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.audit_logs.lock().unwrap().push(entry.clone());
        Ok(())
    }

    async fn list_versions(&self) -> Result<Vec<PricingConfig>, Box<dyn Error + Send + Sync>> {
        let mut vals: Vec<_> = self.configs.lock().unwrap().values().cloned().collect();
        vals.sort_by_key(|c| c.version);
        Ok(vals)
    }
}
