use application::ports::PricingConfigRepository;
use async_trait::async_trait;
use domain::pricing::PricingConfig;
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Default)]
pub struct InMemoryPricingConfigRepository {
    configs: Arc<Mutex<HashMap<u32, PricingConfig>>>,
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
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.configs
            .lock()
            .unwrap()
            .insert(config.version, config.clone());
        Ok(())
    }

    async fn list_versions(&self) -> Result<Vec<PricingConfig>, Box<dyn Error + Send + Sync>> {
        let mut vals: Vec<_> = self.configs.lock().unwrap().values().cloned().collect();
        vals.sort_by_key(|c| c.version);
        Ok(vals)
    }
}
