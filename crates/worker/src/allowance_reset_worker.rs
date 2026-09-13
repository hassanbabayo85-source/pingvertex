//! GAP 3 Kashi D: monthly fair-use allowance reset worker.
//!
//! The allowance repository performs a lazy reset on the first SMS of a
//! new month, so this worker is a **backstop**: it walks every tenant,
//! recomputes the current cycle's `sms_limit` from the tenant's enabled
//! monitor count, and rolls the cycle forward if it is stale.
//!
//! Without this worker, a tenant that never sends an SMS would keep a
//! stale `cycle_ends_at` forever, and the dashboard (Kashi D — D4) would
//! render an out-of-date allowance. With it, every tenant's row is at
//! most one tick old.

use application::ports::{
    MonitorRepository, TenantAlertAllowanceRepository, TenantRepository,
};
use chrono::Utc;
use std::{error::Error, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub struct AllowanceResetWorker {
    tenant_repository: Arc<dyn TenantRepository>,
    monitor_repository: Arc<dyn MonitorRepository>,
    allowance_repository: Arc<dyn TenantAlertAllowanceRepository>,
    interval: Duration,
}

impl AllowanceResetWorker {
    pub fn new(
        tenant_repository: Arc<dyn TenantRepository>,
        monitor_repository: Arc<dyn MonitorRepository>,
        allowance_repository: Arc<dyn TenantAlertAllowanceRepository>,
        interval: Duration,
    ) -> Self {
        Self {
            tenant_repository,
            monitor_repository,
            allowance_repository,
            interval,
        }
    }

    pub async fn run_once(&self) -> Result<()> {
        let tenants = match self.tenant_repository.list_all().await {
            Ok(ts) => ts,
            Err(err) => {
                warn!(error = %err, "allowance reset: failed to list tenants");
                return Ok(());
            }
        };

        let now = Utc::now();
        let mut refreshed = 0u64;
        let mut failed = 0u64;

        for tenant in tenants {
            let enabled = match self
                .monitor_repository
                .count_enabled_for_tenant(tenant.id)
                .await
            {
                Ok(n) => n,
                Err(err) => {
                    warn!(
                        tenant_id = %tenant.id.0,
                        error = %err,
                        "allowance reset: failed to count enabled monitors; skipping tenant"
                    );
                    failed += 1;
                    continue;
                }
            };

            match self
                .allowance_repository
                .current_cycle(tenant.id, enabled, now)
                .await
            {
                Ok(_) => refreshed += 1,
                Err(err) => {
                    warn!(
                        tenant_id = %tenant.id.0,
                        error = %err,
                        "allowance reset: current_cycle failed"
                    );
                    failed += 1;
                }
            }
        }

        if refreshed > 0 || failed > 0 {
            info!(
                refreshed,
                failed,
                "allowance reset sweep completed"
            );
        }
        Ok(())
    }

    pub async fn run_forever(&self, cancel: CancellationToken) -> Result<()> {
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("allowance reset worker cancellation requested; shutting down");
                    return Ok(());
                }
                _ = ticker.tick() => {
                    if let Err(err) = self.run_once().await {
                        error!(error = %err, "allowance reset sweep failed");
                    }
                }
            }
        }
    }
}
