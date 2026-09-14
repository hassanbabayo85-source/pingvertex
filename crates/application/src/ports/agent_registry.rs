use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use domain::Agent;

pub type AgentRegistryResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Server-side source of truth for internal agent identity.
///
/// This is what the `agent_auth` middleware consults on every internal
/// agent request. `find_by_id` returns `None` for unknown agents — the
/// caller MUST treat that as a hard rejection, not a "skip the check".
#[async_trait]
pub trait AgentRegistry: Send + Sync {
    async fn find_by_id(&self, agent_id: &str) -> AgentRegistryResult<Option<Agent>>;

    async fn list_active(&self) -> AgentRegistryResult<Vec<Agent>>;
}

#[async_trait]
impl<T> AgentRegistry for Arc<T>
where
    T: AgentRegistry + ?Sized,
{
    async fn find_by_id(&self, agent_id: &str) -> AgentRegistryResult<Option<Agent>> {
        (**self).find_by_id(agent_id).await
    }

    async fn list_active(&self) -> AgentRegistryResult<Vec<Agent>> {
        (**self).list_active().await
    }
}
