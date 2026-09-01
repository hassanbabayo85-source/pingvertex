use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use application::ports::ApiFlowRepository;
use async_trait::async_trait;
use domain::{
    entities::ApiFlow,
    shared::ids::{ApiFlowId, MonitorId, TenantId},
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

    async fn delete_for_tenant(&self, id: ApiFlowId, tenant_id: TenantId) -> RepoResult<bool> {
        let mut flows = self.flows.write().unwrap();

        let belongs_to_tenant = flows
            .get(&id)
            .map(|flow| flow.tenant_id == tenant_id)
            .unwrap_or(false);

        if belongs_to_tenant {
            Ok(flows.remove(&id).is_some())
        } else {
            Ok(false)
        }
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
