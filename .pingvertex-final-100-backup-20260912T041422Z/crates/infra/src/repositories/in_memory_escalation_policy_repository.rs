use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, RwLock},
};

use application::ports::EscalationPolicyRepository;
use async_trait::async_trait;
use domain::EscalationPolicy;
use domain::shared::ids::{EscalationPolicyId, MonitorId, TenantId};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Simple in-memory escalation policy repository for development and
/// tests — mirrors `PostgresEscalationPolicyRepository`.
#[derive(Debug, Clone, Default)]
pub struct InMemoryEscalationPolicyRepository {
    policies: Arc<RwLock<HashMap<EscalationPolicyId, EscalationPolicy>>>,
}

impl InMemoryEscalationPolicyRepository {
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl EscalationPolicyRepository for InMemoryEscalationPolicyRepository {
    async fn create(&self, policy: &EscalationPolicy) -> RepoResult<()> {
        self.policies
            .write()
            .unwrap()
            .insert(policy.id, policy.clone());
        Ok(())
    }

    async fn update(&self, policy: &EscalationPolicy) -> RepoResult<()> {
        self.policies
            .write()
            .unwrap()
            .insert(policy.id, policy.clone());
        Ok(())
    }

    async fn find_effective(
        &self,
        monitor_id: MonitorId,
        tenant_id: TenantId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        let policies = self.policies.read().unwrap();

        // Try monitor-specific first, newest wins.
        let monitor_specific = policies
            .values()
            .filter(|policy| policy.monitor_id == Some(monitor_id) && policy.tenant_id == tenant_id)
            .max_by_key(|policy| policy.created_at);

        if let Some(policy) = monitor_specific {
            return Ok(Some(policy.clone()));
        }

        // Fall back to the user's global policy (monitor_id IS NULL).
        Ok(policies
            .values()
            .filter(|policy| policy.monitor_id.is_none() && policy.tenant_id == tenant_id)
            .max_by_key(|policy| policy.created_at)
            .cloned())
    }

    async fn find_by_id(&self, id: EscalationPolicyId) -> RepoResult<Option<EscalationPolicy>> {
        Ok(self.policies.read().unwrap().get(&id).cloned())
    }

    async fn list_for_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<EscalationPolicy>> {
        let mut result: Vec<EscalationPolicy> = self
            .policies
            .read()
            .unwrap()
            .values()
            .filter(|policy| policy.tenant_id == tenant_id)
            .cloned()
            .collect();

        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(result)
    }
}
