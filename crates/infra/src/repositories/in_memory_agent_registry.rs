use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
};

use application::ports::AgentRegistry;
use async_trait::async_trait;
use domain::{Agent, AgentStatus};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Default)]
struct State {
    agents: HashMap<String, Agent>,
}

#[derive(Clone, Default)]
pub struct InMemoryAgentRegistry {
    state: Arc<Mutex<State>>,
}

impl InMemoryAgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or overwrites an agent. Tests use this to seed the
    /// trust boundary before sending requests.
    pub fn register(&self, agent: Agent) {
        self.state
            .lock()
            .unwrap()
            .agents
            .insert(agent.agent_id.clone(), agent);
    }

    /// Marks an agent as revoked. Used by tests to exercise the
    /// revocation path; production revokes via direct SQL.
    pub fn revoke(&self, agent_id: &str) {
        if let Some(agent) = self.state.lock().unwrap().agents.get_mut(agent_id) {
            agent.status = AgentStatus::Revoked;
            agent.revoked_at = Some(chrono::Utc::now());
            agent.updated_at = chrono::Utc::now();
        }
    }
}

#[async_trait]
impl AgentRegistry for InMemoryAgentRegistry {
    async fn find_by_id(&self, agent_id: &str) -> Result<Option<Agent>> {
        Ok(self.state.lock().unwrap().agents.get(agent_id).cloned())
    }

    async fn list_active(&self) -> Result<Vec<Agent>> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .agents
            .values()
            .filter(|a| a.is_active())
            .cloned()
            .collect())
    }
}
