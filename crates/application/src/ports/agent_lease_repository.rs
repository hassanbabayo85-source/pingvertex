use std::{error::Error, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::RegionCode;
use domain::{
    Check,
    shared::ids::{CheckId, MonitorId},
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLease {
    pub check_id: CheckId,
    pub monitor_id: MonitorId,
    pub region: RegionCode,
    pub lease_expires_at: DateTime<Utc>,
    pub billing_reserved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitAgentResultOutcome {
    Accepted,
    Duplicate,
    Stale,
}

#[async_trait]
pub trait AgentLeaseRepository: Send + Sync {
    async fn claim_due_checks(
        &self,
        agent_id: &str,
        region: &RegionCode,
        limit: u32,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<AgentLease>>;

    async fn submit_result_with_check(
        &self,
        agent_id: &str,
        check_id: CheckId,
        monitor_id: MonitorId,
        region: &RegionCode,
        now: DateTime<Utc>,
        check: &Check,
    ) -> RepoResult<SubmitAgentResultOutcome>;

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64>;

    async fn mark_billing_reserved(&self, check_id: CheckId) -> RepoResult<()>;

    async fn billing_reserved(&self, check_id: CheckId) -> RepoResult<bool>;
}

#[async_trait]
impl<T> AgentLeaseRepository for Arc<T>
where
    T: AgentLeaseRepository + ?Sized + Send + Sync,
{
    async fn claim_due_checks(
        &self,
        agent_id: &str,
        region: &RegionCode,
        limit: u32,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<AgentLease>> {
        (**self)
            .claim_due_checks(agent_id, region, limit, lease_duration, now)
            .await
    }

    async fn submit_result_with_check(
        &self,
        agent_id: &str,
        check_id: CheckId,
        monitor_id: MonitorId,
        region: &RegionCode,
        now: DateTime<Utc>,
        check: &Check,
    ) -> RepoResult<SubmitAgentResultOutcome> {
        (**self)
            .submit_result_with_check(agent_id, check_id, monitor_id, region, now, check)
            .await
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64> {
        (**self).recover_expired_leases(now).await
    }

    async fn mark_billing_reserved(&self, check_id: CheckId) -> RepoResult<()> {
        (**self).mark_billing_reserved(check_id).await
    }

    async fn billing_reserved(&self, check_id: CheckId) -> RepoResult<bool> {
        (**self).billing_reserved(check_id).await
    }
}
