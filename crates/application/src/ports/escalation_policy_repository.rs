use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

use domain::EscalationPolicy;
use domain::shared::ids::{EscalationPolicyId, MonitorId, UserId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait EscalationPolicyRepository: Send + Sync {
    async fn create(&self, policy: &EscalationPolicy) -> RepoResult<()>;

    async fn update(&self, policy: &EscalationPolicy) -> RepoResult<()>;

    async fn find_by_id_for_user(
        &self,
        id: EscalationPolicyId,
        user_id: UserId,
    ) -> RepoResult<Option<EscalationPolicy>>;

    /// Returns the effective policy for a monitor: monitor-specific first,
    /// then the user's global policy (monitor_id IS NULL).
    async fn find_effective(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<EscalationPolicy>>;

    async fn list_for_user(&self, user_id: UserId) -> RepoResult<Vec<EscalationPolicy>>;
}

#[async_trait]
impl<T> EscalationPolicyRepository for Arc<T>
where
    T: EscalationPolicyRepository + Send + Sync + ?Sized,
{
    async fn create(&self, policy: &EscalationPolicy) -> RepoResult<()> {
        (**self).create(policy).await
    }

    async fn update(&self, policy: &EscalationPolicy) -> RepoResult<()> {
        (**self).update(policy).await
    }

    async fn find_by_id_for_user(
        &self,
        id: EscalationPolicyId,
        user_id: UserId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        (**self).find_by_id_for_user(id, user_id).await
    }

    async fn find_effective(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        (**self).find_effective(monitor_id, user_id).await
    }

    async fn list_for_user(&self, user_id: UserId) -> RepoResult<Vec<EscalationPolicy>> {
        (**self).list_for_user(user_id).await
    }
}
