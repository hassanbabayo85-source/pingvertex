use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::TenantAlertAllowance;
use domain::shared::ids::TenantId;
use std::{error::Error, sync::Arc};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence for `TenantAlertAllowance`.
///
/// Cycle boundaries are always UTC [1st-of-month 00:00, 1st-of-next-month
/// 00:00). `current_cycle` performs a lazy reset when the stored cycle is
/// stale, so the caller does not need to schedule a reset precisely — the
/// first SMS of a new month rolls the cycle forward.
#[async_trait]
pub trait TenantAlertAllowanceRepository: Send + Sync {
    /// Returns the tenant's allowance for the cycle containing `now`.
    ///
    /// If no row exists, it is created. If the stored row is from an
    /// earlier cycle, its limits and counters are reset. `sms_limit` is
    /// computed from `enabled_monitor_count` on creation/reset only and
    /// frozen for the remainder of the cycle.
    async fn current_cycle(
        &self,
        tenant_id: TenantId,
        enabled_monitor_count: u64,
        now: DateTime<Utc>,
    ) -> RepoResult<TenantAlertAllowance>;

    /// Atomically increments `sms_used` if `sms_used < sms_limit` for the
    /// tenant's current cycle. Returns `true` when the allowance was
    /// consumed, `false` when it was already exhausted.
    async fn try_consume_sms(
        &self,
        tenant_id: TenantId,
        now: DateTime<Utc>,
    ) -> RepoResult<bool>;

    /// Read-only lookup. Returns `None` if the tenant has never had an
    /// allowance row created.
    async fn find(&self, tenant_id: TenantId) -> RepoResult<Option<TenantAlertAllowance>>;
}

#[async_trait]
impl<T> TenantAlertAllowanceRepository for Arc<T>
where
    T: TenantAlertAllowanceRepository + Send + Sync + ?Sized,
{
    async fn current_cycle(
        &self,
        tenant_id: TenantId,
        enabled_monitor_count: u64,
        now: DateTime<Utc>,
    ) -> RepoResult<TenantAlertAllowance> {
        (**self)
            .current_cycle(tenant_id, enabled_monitor_count, now)
            .await
    }

    async fn try_consume_sms(
        &self,
        tenant_id: TenantId,
        now: DateTime<Utc>,
    ) -> RepoResult<bool> {
        (**self).try_consume_sms(tenant_id, now).await
    }

    async fn find(&self, tenant_id: TenantId) -> RepoResult<Option<TenantAlertAllowance>> {
        (**self).find(tenant_id).await
    }
}
