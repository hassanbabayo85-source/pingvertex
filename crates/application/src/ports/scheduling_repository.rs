use std::{error::Error, ops::Deref, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Monitor, shared::ids::MonitorId};
use uuid::Uuid;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// A monitor claim together with the durable claim timestamp. The timestamp
/// is part of the billing idempotency identity: normal completion clears it,
/// while lease recovery deliberately preserves it so a crash/retry reuses the
/// same PAYG request id instead of creating a second charge.
#[derive(Debug, Clone)]
pub struct ClaimedOneTimeCheck {
    pub request: domain::OneTimeCheckRequest,
    pub monitor: Monitor,
}

#[derive(Debug, Clone)]
pub struct ClaimedMonitor {
    pub monitor: Monitor,
    pub claimed_at: DateTime<Utc>,
    /// The scheduler timestamp at which this monitor was due. Used only for
    /// observability; billing identity continues to use `execution_id`.
    pub scheduled_at: DateTime<Utc>,
    pub execution_id: Uuid,
}
impl Deref for ClaimedMonitor {
    type Target = Monitor;

    fn deref(&self) -> &Self::Target {
        &self.monitor
    }
}

/// Database-backed scheduling state for the durable worker scheduler.
///
/// The scheduler loop uses this repository to:
/// - atomically claim due monitors (no double-claim under concurrent
///   scheduler instances)
/// - release claims after execution
/// - recover leases left behind by crashed workers.
#[async_trait]
pub trait SchedulingRepository: Send + Sync {
    /// Atomically claims up to `limit` due monitors, assigning
    /// `lease_owner` and a lease that expires after `lease_duration`.
    ///
    /// The returned monitors are guaranteed to be exclusive to the
    /// caller until the lease expires or `complete_claim` is called.
    async fn claim_due_monitors(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedMonitor>>;

    /// Claims due monitors with a hard database-enforced per-tenant cap.
    /// Implementations may fall back to the legacy claim method when no cap
    /// is supplied, but production scheduling uses this method.
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

    /// Returns the number of active monitors that are currently due and
    /// eligible for scheduling. This is exposed as a backpressure metric.
    async fn due_monitor_count(&self, _now: DateTime<Utc>) -> RepoResult<u64> {
        Ok(0)
    }

    /// Clears the lease fields for `monitor_id` and advances
    /// `next_due_at` by `interval_seconds` from the database's `now()`.
    /// Called after an execution attempt finishes (success or error).
    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()>;

    /// Completes a claim and applies a deterministic, bounded schedule spread.
    async fn complete_claim_with_spread(
        &self,
        monitor_id: MonitorId,
        lease_owner: Uuid,
        spread_seconds: u64,
    ) -> RepoResult<()> {
        let _ = spread_seconds;
        self.complete_claim(monitor_id, lease_owner).await
    }

    /// Marks all eligible monitors in a tenant due immediately without
    /// disturbing an active lease. The worker scheduler will claim them on
    /// its next pass.
    async fn request_immediate_checks(
        &self,
        tenant_id: domain::shared::ids::TenantId,
    ) -> RepoResult<u64>;

    /// Clears leases whose `lease_expires_at` is earlier than `now`,
    /// making those monitors claimable again. Returns the number of
    /// recovered leases.
    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64>;

    /// Extends an active scheduler lease for its current owner.
    /// Recovered, expired, or differently-owned leases must not be resurrected.
    async fn renew_lease(
        &self,
        monitor_id: MonitorId,
        lease_owner: Uuid,
        extend_by: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<bool>;

    /// Persists a PAYG billing block for a claimed monitor so repeated
    /// scheduler loops do not keep attempting a check that cannot be funded.
    async fn set_billing_block(
        &self,
        monitor_id: MonitorId,
        tenant_id: domain::shared::ids::TenantId,
        reason: &str,
    ) -> RepoResult<bool>;
    async fn enqueue_one_time_check(
        &self,
        tenant_id: domain::shared::ids::TenantId,
        monitor_id: MonitorId,
        created_by: domain::shared::ids::UserId,
        request_id: &str,
    ) -> RepoResult<domain::OneTimeCheckRequest>;
    async fn claim_one_time_checks(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedOneTimeCheck>>;
    async fn complete_one_time_check(
        &self,
        request_id: Uuid,
        lease_owner: Uuid,
        status: &str,
        error: Option<&str>,
    ) -> RepoResult<()>;
    async fn get_one_time_check(
        &self,
        tenant_id: domain::shared::ids::TenantId,
        monitor_id: MonitorId,
        request_id: Uuid,
    ) -> RepoResult<Option<domain::OneTimeCheckRequest>>;

    /// Looks up a one-time request by its caller-supplied idempotency key.
    /// The key is tenant-scoped and may be an opaque string; it is never
    /// interpreted as a UUID.
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
    ) -> RepoResult<domain::OneTimeCheckRequest> {
        (**self).enqueue_one_time_check(t, m, u, r).await
    }
    async fn claim_one_time_checks(
        &self,
        l: u32,
        o: Uuid,
        d: Duration,
        n: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedOneTimeCheck>> {
        (**self).claim_one_time_checks(l, o, d, n).await
    }
    async fn complete_one_time_check(
        &self,
        r: Uuid,
        o: Uuid,
        s: &str,
        e: Option<&str>,
    ) -> RepoResult<()> {
        (**self).complete_one_time_check(r, o, s, e).await
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
