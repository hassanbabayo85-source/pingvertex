use std::{error::Error, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Monitor, shared::ids::MonitorId};
use uuid::Uuid;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

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
    ) -> RepoResult<Vec<Monitor>>;

    /// Clears the lease fields for `monitor_id` and advances
    /// `next_due_at` by `interval_seconds` from the database's `now()`.
    /// Called after an execution attempt finishes (success or error).
    async fn complete_claim(&self, monitor_id: MonitorId) -> RepoResult<()>;

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
    ) -> RepoResult<Vec<Monitor>> {
        (**self)
            .claim_due_monitors(limit, lease_owner, lease_duration, now)
            .await
    }

    async fn complete_claim(&self, monitor_id: MonitorId) -> RepoResult<()> {
        (**self).complete_claim(monitor_id).await
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
