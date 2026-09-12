use std::{error::Error, ops::Deref, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Monitor, shared::ids::MonitorId};
use uuid::Uuid;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// A claimed one-time regional execution together with its parent request
/// and the monitor snapshot needed to run the check.
#[derive(Debug, Clone)]
pub struct ClaimedOneTimeExecution {
    pub execution: domain::OneTimeCheckExecution,
    pub request: domain::OneTimeCheckRequest,
    pub monitor: Monitor,
}

#[derive(Debug, Clone)]
pub struct ClaimedMonitor {
    pub monitor: Monitor,
    pub claimed_at: DateTime<Utc>,
    pub scheduled_at: DateTime<Utc>,
    pub execution_id: Uuid,
}
impl Deref for ClaimedMonitor {
    type Target = Monitor;
    fn deref(&self) -> &Self::Target {
        &self.monitor
    }
}

#[async_trait]
pub trait SchedulingRepository: Send + Sync {
    async fn claim_due_monitors(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedMonitor>>;

    async fn claim_due_monitors_with_limits(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
        max_concurrency_per_tenant: usize,
        due_claim_limit_per_tenant: usize,
    ) -> RepoResult<Vec<ClaimedMonitor>> {
        let _ = (max_concurrency_per_tenant, due_claim_limit_per_tenant);
        self.claim_due_monitors(limit, lease_owner, lease_duration, now)
            .await
    }

    async fn due_monitor_count(&self, _now: DateTime<Utc>) -> RepoResult<u64> {
        Ok(0)
    }

    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()>;

    async fn complete_claim_with_spread(
        &self,
        monitor_id: MonitorId,
        lease_owner: Uuid,
        spread_seconds: u64,
    ) -> RepoResult<()> {
        let _ = spread_seconds;
        self.complete_claim(monitor_id, lease_owner).await
    }

    async fn request_immediate_checks(
        &self,
        tenant_id: domain::shared::ids::TenantId,
    ) -> RepoResult<u64>;

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64>;

    async fn renew_lease(
        &self,
        monitor_id: MonitorId,
        lease_owner: Uuid,
        extend_by: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<bool>;

    async fn set_billing_block(
        &self,
        monitor_id: MonitorId,
        tenant_id: domain::shared::ids::TenantId,
        reason: &str,
    ) -> RepoResult<bool>;

    /// Persists a one-time check request together with one QUEUED execution
    /// per supplied region. Idempotent on `(tenant_id, request_id)`.
    async fn enqueue_one_time_check(
        &self,
        tenant_id: domain::shared::ids::TenantId,
        monitor_id: MonitorId,
        created_by: domain::shared::ids::UserId,
        request_id: &str,
        regions: &[domain::RegionCode],
    ) -> RepoResult<domain::OneTimeCheckRequest>;

    /// Atomically claims up to `limit` QUEUED (or lease-expired RUNNING)
    /// one-time regional executions.
    async fn claim_one_time_executions(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedOneTimeExecution>>;

    /// Finalizes a claimed execution and rolls up the parent request status.
    #[allow(clippy::too_many_arguments)]
    async fn complete_one_time_execution(
        &self,
        execution_id: Uuid,
        lease_owner: Uuid,
        status: &str,
        error: Option<&str>,
        attempted: bool,
        billable: bool,
        charge_units: i64,
    ) -> RepoResult<()>;

    /// Returns every regional execution belonging to a request, ordered by
    /// region for stable display.
    async fn get_one_time_check_executions(
        &self,
        request_id: Uuid,
    ) -> RepoResult<Vec<domain::OneTimeCheckExecution>>;

    async fn get_one_time_check(
        &self,
        tenant_id: domain::shared::ids::TenantId,
        monitor_id: MonitorId,
        request_id: Uuid,
    ) -> RepoResult<Option<domain::OneTimeCheckRequest>>;

    async fn get_one_time_check_by_request_id(
        &self,
        tenant_id: domain::shared::ids::TenantId,
        monitor_id: MonitorId,
        request_id: &str,
    ) -> RepoResult<Option<domain::OneTimeCheckRequest>>;
}

#[async_trait]
impl<T> SchedulingRepository for Arc<T>
where
    T: SchedulingRepository + ?Sized + Send + Sync,
{
    async fn claim_due_monitors(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedMonitor>> {
        (**self)
            .claim_due_monitors(limit, lease_owner, lease_duration, now)
            .await
    }

    async fn claim_due_monitors_with_limits(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
        max_concurrency_per_tenant: usize,
        due_claim_limit_per_tenant: usize,
    ) -> RepoResult<Vec<ClaimedMonitor>> {
        (**self)
            .claim_due_monitors_with_limits(
                limit,
                lease_owner,
                lease_duration,
                now,
                max_concurrency_per_tenant,
                due_claim_limit_per_tenant,
            )
            .await
    }

    async fn due_monitor_count(&self, now: DateTime<Utc>) -> RepoResult<u64> {
        (**self).due_monitor_count(now).await
    }

    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()> {
        (**self).complete_claim(monitor_id, lease_owner).await
    }

    async fn complete_claim_with_spread(
        &self,
        monitor_id: MonitorId,
        lease_owner: Uuid,
        spread_seconds: u64,
    ) -> RepoResult<()> {
        (**self)
            .complete_claim_with_spread(monitor_id, lease_owner, spread_seconds)
            .await
    }

    async fn request_immediate_checks(
        &self,
        tenant_id: domain::shared::ids::TenantId,
    ) -> RepoResult<u64> {
        (**self).request_immediate_checks(tenant_id).await
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64> {
        (**self).recover_expired_leases(now).await
    }

    async fn set_billing_block(
        &self,
        monitor_id: MonitorId,
        tenant_id: domain::shared::ids::TenantId,
        reason: &str,
    ) -> RepoResult<bool> {
        (**self)
            .set_billing_block(monitor_id, tenant_id, reason)
            .await
    }

    async fn enqueue_one_time_check(
        &self,
        t: domain::shared::ids::TenantId,
        m: MonitorId,
        u: domain::shared::ids::UserId,
        r: &str,
        regions: &[domain::RegionCode],
    ) -> RepoResult<domain::OneTimeCheckRequest> {
        (**self).enqueue_one_time_check(t, m, u, r, regions).await
    }

    async fn claim_one_time_executions(
        &self,
        l: u32,
        o: Uuid,
        d: Duration,
        n: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedOneTimeExecution>> {
        (**self).claim_one_time_executions(l, o, d, n).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_one_time_execution(
        &self,
        e: Uuid,
        o: Uuid,
        s: &str,
        err: Option<&str>,
        attempted: bool,
        billable: bool,
        charge_units: i64,
    ) -> RepoResult<()> {
        (**self)
            .complete_one_time_execution(e, o, s, err, attempted, billable, charge_units)
            .await
    }

    async fn get_one_time_check_executions(
        &self,
        r: Uuid,
    ) -> RepoResult<Vec<domain::OneTimeCheckExecution>> {
        (**self).get_one_time_check_executions(r).await
    }

    async fn get_one_time_check(
        &self,
        t: domain::shared::ids::TenantId,
        m: MonitorId,
        r: Uuid,
    ) -> RepoResult<Option<domain::OneTimeCheckRequest>> {
        (**self).get_one_time_check(t, m, r).await
    }

    async fn get_one_time_check_by_request_id(
        &self,
        t: domain::shared::ids::TenantId,
        m: MonitorId,
        r: &str,
    ) -> RepoResult<Option<domain::OneTimeCheckRequest>> {
        (**self).get_one_time_check_by_request_id(t, m, r).await
    }

    async fn renew_lease(
        &self,
        monitor_id: MonitorId,
        lease_owner: Uuid,
        extend_by: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<bool> {
        (**self)
            .renew_lease(monitor_id, lease_owner, extend_by, now)
            .await
    }
}
