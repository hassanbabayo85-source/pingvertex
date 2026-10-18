//! Persistence port for recurring proxy monitors.
//!
//! # Design notes
//!
//! * **Credentials.** `RecurringProxyMonitor::credentials` is an
//!   `Option<ProxyCredentials>` that is *not* `Clone` by design (see
//!   `domain::value_objects::ProxyCredentials`). The repository must
//!   therefore never cache the entity itself — every `find_*` call
//!   rebuilds the entity from stored fields, decrypting credentials
//!   when present. This mirrors `ProxyCheckJobRepository`'s pattern.
//!
//! * **Leases.** A recurring monitor is scheduled, not executed
//!   synchronously. `claim_due_monitors` atomically leases a batch of
//!   due monitors to a specific worker, returning them together with
//!   the lease metadata. `reschedule` releases the lease and advances
//!   `next_run_at`. Both operations are per-tenant scoped: a monitor
//!   belonging to tenant A can never be claimed or rescheduled by
//!   tenant B.
//!
//! * **Quota.** `save_monitor_if_within_quota` uses the same
//!   advisory-lock pattern as `MonitorRepository::save_if_within_quota`
//!   (see that trait for the detailed rationale). The limit comes from
//!   `Entitlements::max_recurring_proxy_monitors`, resolved by the
//!   caller before invoking this method.
//!
//! * **History.** Each successful or failed execution is recorded as a
//!   `RecurringProxyCheck` row. Checks are scoped by the parent
//!   monitor's tenant.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::{RecurringProxyCheck, RecurringProxyMonitor};
use domain::shared::ids::TenantId;
use uuid::Uuid;

use super::monitor_repository::QuotaCheckOutcome;
use super::RepoResult;

/// A monitor that a worker has just claimed.
///
/// Carries the entity plus the lease metadata that the worker needs to
/// keep the claim alive. Keeping the lease fields **outside** the
/// entity means the entity stays a pure "what the customer sees"
/// object and never has to be reconstructed with internal worker state.
#[derive(Debug)]
pub struct ClaimedRecurringProxyMonitor {
    pub monitor: RecurringProxyMonitor,
    pub worker_id: String,
    pub lease_expires_at: DateTime<Utc>,
    pub claimed_at: DateTime<Utc>,
}

#[async_trait]
pub trait RecurringProxyMonitorRepository: Send + Sync {
    // ── Monitor CRUD ────────────────────────────────────────────

    /// Persists a new monitor. Credentials, if present, are encrypted
    /// by the implementation before being written to durable storage.
    async fn save_monitor(&self, monitor: &RecurringProxyMonitor) -> RepoResult<()>;

    /// Atomically checks the tenant's current monitor count against
    /// `limit` and inserts only if strictly under it. Mirrors
    /// `MonitorRepository::save_if_within_quota`'s contract.
    async fn save_monitor_if_within_quota(
        &self,
        monitor: &RecurringProxyMonitor,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome>;

    async fn find_by_id(
        &self,
        id: Uuid,
        tenant_id: TenantId,
    ) -> RepoResult<Option<RecurringProxyMonitor>>;

    async fn list_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<RecurringProxyMonitor>>;

    async fn update_monitor(&self, monitor: &RecurringProxyMonitor) -> RepoResult<()>;

    async fn delete_for_tenant(&self, id: Uuid, tenant_id: TenantId) -> RepoResult<bool>;

    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64>;

    // ── Worker claim / release ──────────────────────────────────

    /// Atomically leases up to `limit` due monitors to `worker_id` for
    /// `lease_for`. A monitor is "due" when all of:
    ///   * `user_enabled = TRUE`,
    ///   * `entitlement_suspended = FALSE`,
    ///   * `next_run_at IS NOT NULL AND next_run_at <= now()`,
    ///   * `lease_expires_at IS NULL OR lease_expires_at < now()`.
    ///
    /// Rows returned by this method are locked for the duration of the
    /// transaction that writes the lease; concurrent callers cannot
    /// receive the same monitor twice.
    async fn claim_due_monitors(
        &self,
        now: DateTime<Utc>,
        worker_id: &str,
        lease_until: DateTime<Utc>,
        limit: u32,
    ) -> RepoResult<Vec<ClaimedRecurringProxyMonitor>>;

    /// Releases the worker's lease on `id` and advances
    /// `last_run_at` / `next_run_at`. Idempotent: returns `Ok(false)`
    /// if the caller does not hold the lease, without erroring — a
    /// worker that was declared dead and replaced must not be able to
    /// reschedule a monitor that another worker now owns.
    async fn reschedule(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        worker_id: &str,
        last_run_at: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> RepoResult<bool>;

    /// Releases the lease without advancing the schedule. Used when a
    /// worker fails to complete a check (crash, cancellation) and wants
    /// the monitor re-claimable.
    async fn release_lease(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        worker_id: &str,
    ) -> RepoResult<bool>;

    // ── Per-execution history ───────────────────────────────────

    /// Inserts a `RecurringProxyCheck` row. The repository sets the
    /// `started_at` if not already populated by the caller.
    async fn save_check(&self, check: &RecurringProxyCheck) -> RepoResult<()>;

    /// Appends the terminal status, `finished_at`, result, and error to
    /// a check row. Returns `Ok(false)` if the row doesn't exist (e.g.
    /// was deleted in the meantime).
    async fn finish_check(&self, check: &RecurringProxyCheck) -> RepoResult<bool>;

    async fn list_checks(
        &self,
        monitor_id: Uuid,
        tenant_id: TenantId,
        offset: u32,
        limit: u32,
    ) -> RepoResult<(Vec<RecurringProxyCheck>, u64)>;

    async fn find_latest_check(
        &self,
        monitor_id: Uuid,
        tenant_id: TenantId,
    ) -> RepoResult<Option<RecurringProxyCheck>>;
}

#[async_trait]
impl<T> RecurringProxyMonitorRepository for std::sync::Arc<T>
where
    T: RecurringProxyMonitorRepository + ?Sized,
{
    async fn save_monitor(&self, monitor: &RecurringProxyMonitor) -> RepoResult<()> {
        (**self).save_monitor(monitor).await
    }
    async fn save_monitor_if_within_quota(
        &self,
        monitor: &RecurringProxyMonitor,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        (**self)
            .save_monitor_if_within_quota(monitor, tenant_id, limit)
            .await
    }
    async fn find_by_id(
        &self,
        id: Uuid,
        tenant_id: TenantId,
    ) -> RepoResult<Option<RecurringProxyMonitor>> {
        (**self).find_by_id(id, tenant_id).await
    }
    async fn list_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<RecurringProxyMonitor>> {
        (**self).list_for_tenant(tenant_id).await
    }
    async fn update_monitor(&self, monitor: &RecurringProxyMonitor) -> RepoResult<()> {
        (**self).update_monitor(monitor).await
    }
    async fn delete_for_tenant(&self, id: Uuid, tenant_id: TenantId) -> RepoResult<bool> {
        (**self).delete_for_tenant(id, tenant_id).await
    }
    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        (**self).count_for_tenant(tenant_id).await
    }
    async fn claim_due_monitors(
        &self,
        now: DateTime<Utc>,
        worker_id: &str,
        lease_until: DateTime<Utc>,
        limit: u32,
    ) -> RepoResult<Vec<ClaimedRecurringProxyMonitor>> {
        (**self)
            .claim_due_monitors(now, worker_id, lease_until, limit)
            .await
    }
    async fn reschedule(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        worker_id: &str,
        last_run_at: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> RepoResult<bool> {
        (**self)
            .reschedule(id, tenant_id, worker_id, last_run_at, next_run_at)
            .await
    }
    async fn release_lease(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        worker_id: &str,
    ) -> RepoResult<bool> {
        (**self).release_lease(id, tenant_id, worker_id).await
    }
    async fn save_check(&self, check: &RecurringProxyCheck) -> RepoResult<()> {
        (**self).save_check(check).await
    }
    async fn finish_check(&self, check: &RecurringProxyCheck) -> RepoResult<bool> {
        (**self).finish_check(check).await
    }
    async fn list_checks(
        &self,
        monitor_id: Uuid,
        tenant_id: TenantId,
        offset: u32,
        limit: u32,
    ) -> RepoResult<(Vec<RecurringProxyCheck>, u64)> {
        (**self)
            .list_checks(monitor_id, tenant_id, offset, limit)
            .await
    }
    async fn find_latest_check(
        &self,
        monitor_id: Uuid,
        tenant_id: TenantId,
    ) -> RepoResult<Option<RecurringProxyCheck>> {
        (**self).find_latest_check(monitor_id, tenant_id).await
    }
}
