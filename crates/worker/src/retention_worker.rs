use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use application::{
    ports::{HistoryRetentionRepository, SubscriptionRepository, TenantRepository},
    queries::ResolveEntitlements,
    use_cases::resolve_entitlements::ResolveEntitlementsUseCase,
};
use chrono::{Duration as ChronoDuration, Utc};
use tokio::time;
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
    S: SubscriptionRepository,
    H: HistoryRetentionRepository,
{
    tenant_repository: Arc<T>,
    subscription_repository: Arc<S>,
    history_repository: Arc<H>,
    batch_size: u32,
    interval: Duration,
}

impl<T, S, H> RetentionWorker<T, S, H>
where
    T: TenantRepository,
    S: SubscriptionRepository,
    H: HistoryRetentionRepository,
{
    pub fn new(
        tenant_repository: Arc<T>,
        subscription_repository: Arc<S>,
        history_repository: Arc<H>,
        batch_size: u32,
        interval: Duration,
    ) -> Self {
        Self {
            tenant_repository,
            subscription_repository,
            history_repository,
            batch_size,
            interval,
        }
    }

    /// Runs one full retention sweep over all tenants. Failures for an
    /// individual tenant are logged and skipped so one bad tenant cannot
    /// stop the whole background worker.
    pub async fn run_sweep(&self) -> Result<SweepSummary, Box<dyn Error + Send + Sync>> {
        let tenants = self.tenant_repository.list_all().await?;
        let mut summary = SweepSummary::default();

        for tenant in tenants {
            let plan = match ResolveEntitlementsUseCase::new(self.subscription_repository.clone())
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
                    continue;
                }
            };

            let retention_days = plan.history_retention_days;
            let cutoff = Utc::now() - ChronoDuration::days(retention_days as i64);

            match self
                .history_repository
                .delete_older_than_batched(tenant.id, cutoff, self.batch_size)
                .await
            {
                Ok(deleted) => {
                    summary.tenants_processed += 1;
                    if deleted > 0 {
                        summary.checks_deleted += deleted;
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
        }

        Ok(summary)
    }

    pub async fn run_forever(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!(
            interval_secs = self.interval.as_secs(),
            batch_size = self.batch_size,
            "retention worker started"
        );

        let mut ticker = time::interval(self.interval);
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;
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
