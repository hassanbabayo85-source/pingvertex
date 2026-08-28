use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use application::{
    ports::{HistoryRetentionRepository, SubscriptionRepository, TenantRepository},
    queries::ResolveEntitlements,
    use_cases::resolve_entitlements::ResolveEntitlementsUseCase,
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

pub struct RetentionWorker<T, S, H>
where
    T: TenantRepository,
    S: SubscriptionRepository + 'static,
    H: HistoryRetentionRepository + 'static,
{
    tenant_repository: Arc<T>,
    subscription_repository: Arc<S>,
    history_repository: Arc<H>,
    batch_size: u32,
    interval: Duration,
    max_concurrent_tenants: usize,
}

impl<T, S, H> RetentionWorker<T, S, H>
where
    T: TenantRepository,
    S: SubscriptionRepository + 'static,
    H: HistoryRetentionRepository + 'static,
{
    pub fn new(
        tenant_repository: Arc<T>,
        subscription_repository: Arc<S>,
        history_repository: Arc<H>,
        batch_size: u32,
        interval: Duration,
        max_concurrent_tenants: usize,
    ) -> Self {
        Self {
            tenant_repository,
            subscription_repository,
            history_repository,
            batch_size,
            interval,
            max_concurrent_tenants,
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
            let subscription_repository = self.subscription_repository.clone();
            let history_repository = self.history_repository.clone();
            let batch_size = self.batch_size;
            let checks_deleted = checks_deleted.clone();

            join_set.spawn(async move {
                let _permit = permit;

                let plan = match ResolveEntitlementsUseCase::new(subscription_repository)
                    .execute(ResolveEntitlements {
                        tenant_id: tenant.id,
                    })
                    .await
                {
                    Ok(plan) => plan,
                    Err(err) => {
                        error!(
                            tenant_id = ?tenant.id,
                            error = %err,
                            "failed to resolve entitlements for tenant; skipping retention"
                        );
                        return;
                    }
                };

                let retention_days = plan.history_retention_days;
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
