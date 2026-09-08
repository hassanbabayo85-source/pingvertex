use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::{
    entities::Monitor,
    shared::ids::{MonitorId, TenantId},
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Result of an atomic, quota-checked resource creation via
/// [`MonitorRepository::save_if_within_quota`].
///
/// Establishes the naming/shape pattern future quota-checked repository
/// methods (status pages, API keys, team members, notifications, ...)
/// should mirror when B13's atomic-quota pattern is reused for other
/// resource types. See `PostgresMonitorRepository::save_if_within_quota`
/// for the reference implementation and its doc comment for why the
/// mechanism is a per-tenant Postgres advisory lock + transaction-scoped
/// `COUNT`, not `SELECT ... FOR UPDATE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaCheckOutcome {
    /// The resource was saved. The tenant was strictly under `limit` at
    /// the time of the check, atomically with the insert — no concurrent
    /// caller could have observed a stale count.
    Saved,
    /// The tenant was already at or above `limit`; nothing was written.
    QuotaExceeded,
}

#[async_trait]
pub trait MonitorRepository: Send + Sync {
    async fn save(&self, monitor: &Monitor) -> RepoResult<()>;

    async fn update(&self, monitor: &Monitor) -> RepoResult<()>;

    async fn delete(&self, id: MonitorId) -> RepoResult<bool>;

    /// SECURITY (P0): tenant-scoped delete. Deletes the monitor only
    /// if it belongs to `tenant_id`; returns `Ok(false)` (not an
    /// error) if the id doesn't exist or belongs to a different
    /// tenant. Prefer this over `delete` for anything reachable from
    /// an authenticated HTTP request, so tenant isolation is
    /// enforced at the SQL layer itself rather than depending solely
    /// on an application-layer check performed before the call.
    async fn delete_for_tenant(&self, _id: MonitorId, _tenant_id: TenantId) -> RepoResult<bool> {
        unimplemented!()
    }

    async fn find_by_id(&self, id: MonitorId) -> RepoResult<Option<Monitor>>;

    async fn find_all(&self) -> RepoResult<Vec<Monitor>>;

    async fn find_by_id_for_user(
        &self,
        _id: MonitorId,
        _user_id: domain::shared::ids::UserId,
    ) -> RepoResult<Option<Monitor>> {
        unimplemented!()
    }
    async fn find_all_for_user(
        &self,
        _user_id: domain::shared::ids::UserId,
    ) -> RepoResult<Vec<Monitor>> {
        unimplemented!()
    }

    /// Tenant-scoped equivalent of `find_all_for_user`. Returns every
    /// monitor belonging to `tenant_id`, regardless of which member of
    /// the tenant created it. Used once tenants can have more than one
    /// member (see the invite flow), so that every member sees the same
    /// monitors instead of only the ones they personally created.
    async fn find_all_for_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<Monitor>> {
        unimplemented!()
    }
    async fn delete_for_user(
        &self,
        _id: MonitorId,
        _user_id: domain::shared::ids::UserId,
    ) -> RepoResult<bool> {
        unimplemented!()
    }

    /// Returns monitors explicitly marked `is_public = true`, for the
    /// unauthenticated public status page.
    async fn find_public(&self) -> RepoResult<Vec<Monitor>> {
        unimplemented!()
    }

    /// Finds the monitor whose `heartbeat_token` matches `token`, used by
    /// the unauthenticated `GET /heartbeat/{token}` endpoint.
    async fn find_by_heartbeat_token(&self, _token: &str) -> RepoResult<Option<Monitor>> {
        unimplemented!()
    }

    /// Updates `last_heartbeat_at` for the given monitor to `now`.
    async fn update_last_heartbeat_at(
        &self,
        _id: MonitorId,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<()> {
        unimplemented!()
    }

    /// Atomically records the outcome of a single check for `id`:
    /// increments `consecutive_failures` and zeroes `consecutive_successes`
    /// when `success` is `false`, or the reverse when `success` is `true`.
    /// Returns the resulting `(consecutive_failures, consecutive_successes)`
    /// counts after the update, read back from the same statement that
    /// wrote them so callers never act on a stale count under concurrent
    /// calls for the same monitor.
    ///
    /// Used by `worker::IncidentEvaluator` (B23) to decide when a run of
    /// consecutive failures/successes crosses the configured
    /// open/recovery threshold. Defaults to `unimplemented!()`, matching
    /// the other worker-path-only methods above — only
    /// `PostgresMonitorRepository` needs a real implementation, since
    /// this is only ever called from the live check-execution path.
    async fn record_check_outcome(&self, _id: MonitorId, _success: bool) -> RepoResult<(u32, u32)> {
        unimplemented!()
    }

    /// Atomically checks `tenant_id`'s current monitor count against
    /// `limit` and, only if currently strictly under it, saves `monitor`
    /// — both steps inside a single transaction/lock scope, so that any
    /// number of concurrent callers racing this method for the same
    /// tenant can never together exceed `limit`.
    ///
    /// This is B13's answer to the classic
    /// `COUNT -> COMPARE -> INSERT` race: callers must use this instead
    /// of separately calling `find_all_for_tenant`/`find_all_for_user`
    /// to count, comparing in application code, then calling `save`.
    /// See `PostgresMonitorRepository::save_if_within_quota` for the
    /// reference implementation of the underlying mechanism, which is
    /// the reusable pattern intended for future resource types.
    async fn save_if_within_quota(
        &self,
        _monitor: &Monitor,
        _tenant_id: TenantId,
        _limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        unimplemented!()
    }

    /// Atomically enables/disables a tenant's selected monitors. `None`
    /// means the whole tenant. Implementations MUST scope by tenant_id.
    async fn set_enabled_for_tenant(
        &self,
        _tenant_id: TenantId,
        _monitor_ids: Option<&[MonitorId]>,
        _enabled: bool,
    ) -> RepoResult<u64> {
        Ok(0)
    }

    /// Creates or replaces a durable billing block for a monitor. The
    /// monitor must belong to tenant_id; cross-tenant ids are ignored.
    async fn set_billing_block(
        &self,
        _monitor_id: MonitorId,
        _tenant_id: TenantId,
        _reason: &str,
    ) -> RepoResult<bool> {
        Ok(false)
    }

    async fn clear_billing_blocks_for_tenant(&self, _tenant_id: TenantId) -> RepoResult<u64> {
        Ok(0)
    }

    /// Returns billing-block reasons for the tenant in one query, avoiding
    /// N+1 lookups in the monitor list UI.
    async fn billing_blocks_for_tenant(
        &self,
        _tenant_id: TenantId,
    ) -> RepoResult<std::collections::HashMap<MonitorId, String>> {
        Ok(std::collections::HashMap::new())
    }
}

#[async_trait]
impl<T> MonitorRepository for Arc<T>
where
    T: MonitorRepository + Send + Sync + ?Sized,
{
    async fn save(&self, monitor: &Monitor) -> RepoResult<()> {
        (**self).save(monitor).await
    }

    async fn update(&self, monitor: &Monitor) -> RepoResult<()> {
        (**self).update(monitor).await
    }

    async fn delete(&self, id: MonitorId) -> RepoResult<bool> {
        (**self).delete(id).await
    }

    async fn delete_for_tenant(&self, id: MonitorId, tenant_id: TenantId) -> RepoResult<bool> {
        (**self).delete_for_tenant(id, tenant_id).await
    }

    async fn find_by_id(&self, id: MonitorId) -> RepoResult<Option<Monitor>> {
        (**self).find_by_id(id).await
    }

    async fn find_all(&self) -> RepoResult<Vec<Monitor>> {
        (**self).find_all().await
    }

    async fn find_by_id_for_user(
        &self,
        id: MonitorId,
        user_id: domain::shared::ids::UserId,
    ) -> RepoResult<Option<Monitor>> {
        (**self).find_by_id_for_user(id, user_id).await
    }
    async fn find_all_for_user(
        &self,
        user_id: domain::shared::ids::UserId,
    ) -> RepoResult<Vec<Monitor>> {
        (**self).find_all_for_user(user_id).await
    }
    async fn find_all_for_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<Monitor>> {
        (**self).find_all_for_tenant(tenant_id).await
    }
    async fn delete_for_user(
        &self,
        id: MonitorId,
        user_id: domain::shared::ids::UserId,
    ) -> RepoResult<bool> {
        (**self).delete_for_user(id, user_id).await
    }

    async fn find_public(&self) -> RepoResult<Vec<Monitor>> {
        (**self).find_public().await
    }

    async fn find_by_heartbeat_token(&self, token: &str) -> RepoResult<Option<Monitor>> {
        (**self).find_by_heartbeat_token(token).await
    }

    async fn update_last_heartbeat_at(
        &self,
        id: MonitorId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<()> {
        (**self).update_last_heartbeat_at(id, now).await
    }

    async fn record_check_outcome(&self, id: MonitorId, success: bool) -> RepoResult<(u32, u32)> {
        (**self).record_check_outcome(id, success).await
    }

    async fn save_if_within_quota(
        &self,
        monitor: &Monitor,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        (**self)
            .save_if_within_quota(monitor, tenant_id, limit)
            .await
    }

    async fn set_enabled_for_tenant(
        &self,
        tenant_id: TenantId,
        monitor_ids: Option<&[MonitorId]>,
        enabled: bool,
    ) -> RepoResult<u64> {
        (**self).set_enabled_for_tenant(tenant_id, monitor_ids, enabled).await
    }

    async fn set_billing_block(
        &self,
        monitor_id: MonitorId,
        tenant_id: TenantId,
        reason: &str,
    ) -> RepoResult<bool> {
        (**self).set_billing_block(monitor_id, tenant_id, reason).await
    }

    async fn clear_billing_blocks_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        (**self).clear_billing_blocks_for_tenant(tenant_id).await
    }

    async fn billing_blocks_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepoResult<std::collections::HashMap<MonitorId, String>> {
        (**self).billing_blocks_for_tenant(tenant_id).await
    }
}

