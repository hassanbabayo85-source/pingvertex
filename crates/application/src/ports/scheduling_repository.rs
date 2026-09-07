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
pub struct ClaimedMonitor {
    pub monitor: Monitor,
    pub claimed_at: DateTime<Utc>,
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

    /// Clears the lease fields for `monitor_id` and advances
    /// `next_due_at` by `interval_seconds` from the database's `now()`.
    /// Called after an execution attempt finishes (success or error).
    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()>;

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

    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()> {
        (**self).complete_claim(monitor_id, lease_owner).await
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
}
