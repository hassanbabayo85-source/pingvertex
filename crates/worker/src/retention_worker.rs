//! Task 8C — retention sweep driven by `EntitlementResolver`.
//!
//! The per-tenant cutoff is `entitlements.max_history_retention_days`,
//! optionally narrowed by a `TenantRetentionSettings` row (Legacy or
//! Addon source) but never widened above the plan maximum. The PAYG
//! era chain (`WalletRepository.payg_activated_at` + `PricingConfig
//! .free_tier`) has been removed; history preservation is preserved
//! because the worker still only deletes rows older than the cutoff.

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use application::ports::{
    EntitlementResolver, HistoryRetentionRepository, TenantRepository,
    TenantRetentionSettingsRepository,
};
use chrono::{Duration as ChronoDuration, Utc};
use tokio::{sync::Semaphore, task::JoinSet, time};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub const DEFAULT_RETENTION_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
pub const DEFAULT_BATCH_SIZE: u32 = 1000;

/// Result summary for one sweep.
#[derive(Debug, Clone, Default)]
pub struct SweepSummary {
    pub tenants_processed: u64,
    pub checks_deleted: u64,
}

pub struct RetentionWorker<T, H>
where
    T: TenantRepository,
    H: HistoryRetentionRepository + 'static,
{
    tenant_repository: Arc<T>,
    history_repository: Arc<H>,
    entitlement_resolver: Arc<dyn EntitlementResolver>,
    batch_size: u32,
    interval: Duration,
    max_concurrent_tenants: usize,
    retention_settings_repository: Arc<dyn TenantRetentionSettingsRepository>,
}

impl<T, H> RetentionWorker<T, H>
where
    T: TenantRepository,
    H: HistoryRetentionRepository + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_repository: Arc<T>,
        history_repository: Arc<H>,
        entitlement_resolver: Arc<dyn EntitlementResolver>,
        batch_size: u32,
        interval: Duration,
        max_concurrent_tenants: usize,
        retention_settings_repository: Arc<dyn TenantRetentionSettingsRepository>,
    ) -> Self {
        Self {
            tenant_repository,
            history_repository,
            entitlement_resolver,
            batch_size,
            interval,
            max_concurrent_tenants,
            retention_settings_repository,
        }
    }

    /// Runs one full retention sweep over all tenants with bounded
    /// concurrency. Each tenant is processed independently; failures for
    /// an individual tenant are logged and skipped.
    pub async fn run_sweep(&self) -> Result<SweepSummary, Box<dyn Error + Send + Sync>> {
        let tenants = self.tenant_repository.list_all().await?;
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent_tenants.max(1)));
        let mut join_set = JoinSet::new();
        let mut summary = SweepSummary::default();
        let checks_deleted = Arc::new(AtomicU64::new(0));

        for tenant in tenants {
            let permit = semaphore.clone().acquire_owned().await?;
            let history_repository = self.history_repository.clone();
            let entitlement_resolver = self.entitlement_resolver.clone();
            let retention_settings_repository = self.retention_settings_repository.clone();
            let batch_size = self.batch_size;
            let checks_deleted = checks_deleted.clone();

            join_set.spawn(async move {
                let _permit = permit;

                // Task 8C — retention comes from EntitlementResolver.
                // A `TenantRetentionSettings` row (Legacy/Addon) may narrow
                // the window but never widen it above the plan maximum.
                let entitlements = match entitlement_resolver.resolve(tenant.id).await {
                    Ok(e) => e,
                    Err(err) => {
                        error!(
                            tenant_id = ?tenant.id,
                            error = %err,
                            "failed to resolve entitlements for retention; skipping tenant"
                        );
                        return;
                    }
                };
                let max = entitlements.max_history_retention_days.max(1);

                // `effective_retention(tenant, max, max)` yields:
                //   * the tenant row's retention_days if in [1, max],
                //   * otherwise `max` (which we pass as both default and max),
                //   * always clamped to `max`.
                let retention_days = match retention_settings_repository
                    .effective_retention(tenant.id, max, max)
                    .await
                {
                    Ok(days) => days,
                    Err(err) => {
                        error!(
                            tenant_id = ?tenant.id,
                            error = %err,
                            "failed to resolve effective retention; falling back to plan max"
                        );
                        max
                    }
                };

                let cutoff = Utc::now() - ChronoDuration::days(retention_days as i64);

                match history_repository
                    .delete_older_than_batched(tenant.id, cutoff, batch_size)
                    .await
                {
                    Ok(deleted) => {
                        if deleted > 0 {
                            checks_deleted.fetch_add(deleted, Ordering::Relaxed);
                            info!(
                                tenant_id = ?tenant.id,
                                retention_days,
                                deleted,
                                "retention sweep deleted old checks"
                            );
                        }
                    }
                    Err(err) => {
                        error!(
                            tenant_id = ?tenant.id,
                            error = %err,
                            "retention delete failed for tenant"
                        );
                    }
                }
            });
        }

        while join_set.join_next().await.is_some() {
            summary.tenants_processed += 1;
        }

        summary.checks_deleted = checks_deleted.load(Ordering::Relaxed);

        Ok(summary)
    }

    pub async fn run_forever(
        &self,
        token: CancellationToken,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!(
            interval_secs = self.interval.as_secs(),
            batch_size = self.batch_size,
            max_concurrent_tenants = self.max_concurrent_tenants,
            "retention worker started"
        );

        let mut ticker = time::interval(self.interval);
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("retention worker cancellation requested; shutting down");
                    return Ok(());
                }
                _ = ticker.tick() => {
                    match self.run_sweep().await {
                        Ok(summary) => {
                            info!(
                                tenants_processed = summary.tenants_processed,
                                checks_deleted = summary.checks_deleted,
                                "retention sweep completed"
                            );
                        }
                        Err(err) => {
                            error!(error = %err, "retention sweep failed");
                        }
                    }
                }
            }
        }
    }
}
