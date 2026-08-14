use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use application::ports::ApiFlowRepository;
use async_trait::async_trait;
use domain::{
    entities::ApiFlow,
    shared::ids::{ApiFlowId, MonitorId},
};

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone, Default)]
pub struct InMemoryApiFlowRepository {
    flows: Arc<RwLock<HashMap<ApiFlowId, ApiFlow>>>,
}

impl InMemoryApiFlowRepository {
    pub fn new() -> Self {
        Self {
            flows: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl ApiFlowRepository for InMemoryApiFlowRepository {
    async fn save(&self, flow: &ApiFlow) -> RepoResult<()> {
        self.flows.write().unwrap().insert(flow.id, flow.clone());

        Ok(())
    }

    async fn update(&self, flow: &ApiFlow) -> RepoResult<()> {
        self.flows.write().unwrap().insert(flow.id, flow.clone());

        Ok(())
    }

    async fn delete(&self, id: ApiFlowId) -> RepoResult<bool> {
        Ok(self.flows.write().unwrap().remove(&id).is_some())
    }

    async fn find_by_id(&self, id: ApiFlowId) -> RepoResult<Option<ApiFlow>> {
        Ok(self.flows.read().unwrap().get(&id).cloned())
    }

    async fn find_by_monitor_id(&self, monitor_id: MonitorId) -> RepoResult<Option<ApiFlow>> {
        Ok(self
            .flows
            .read()
            .unwrap()
            .values()
            .find(|flow| flow.monitor_id == monitor_id)
            .cloned())
    }
}
