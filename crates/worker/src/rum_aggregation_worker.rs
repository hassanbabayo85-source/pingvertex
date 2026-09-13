//! GAP 3 Kashi E: RUM aggregation worker.
//!
//! Every sweep, walks every tenant and every site that tenant owns and
//! calls `RumRetentionRepository::run_retention_for_site`. That method
//! rolls raw RUM events older than the 7-day retention window into
//! `rum_daily_aggregates` and deletes the raw rows in batches.
//!
//! The aggregate rows are permanent — they are how the dashboard still
//! shows historical RUM data after the raw 7-day window has expired.

use application::ports::{RumRetentionRepository, RumSiteRepository, TenantRepository};
use std::{error::Error, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub const DEFAULT_RUM_RETENTION_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
pub const DEFAULT_RUM_BATCH_SIZE: u32 = 1000;
pub const RUM_RAW_RETENTION_DAYS: i64 = 7;

pub struct RumAggregationWorker {
    tenant_repository: Arc<dyn TenantRepository>,
    site_repository: Arc<dyn RumSiteRepository>,
    retention_repository: Arc<dyn RumRetentionRepository>,
    interval: Duration,
    batch_size: u32,
}

impl RumAggregationWorker {
    pub fn new(
        tenant_repository: Arc<dyn TenantRepository>,
        site_repository: Arc<dyn RumSiteRepository>,
        retention_repository: Arc<dyn RumRetentionRepository>,
        interval: Duration,
        batch_size: u32,
    ) -> Self {
        Self {
            tenant_repository,
            site_repository,
            retention_repository,
            interval,
            batch_size,
        }
    }

    pub async fn run_once(&self) -> Result<()> {
        let tenants = match self.tenant_repository.list_all().await {
            Ok(t) => t,
            Err(err) => {
                error!(error = %err, "rum aggregation: failed to list tenants");
                return Ok(());
            }
        };

        let mut sites_processed = 0u64;
        let mut sites_failed = 0u64;

        for tenant in &tenants {
            let sites = match self.site_repository.list_by_tenant(tenant.id).await {
                Ok(s) => s,
                Err(err) => {
                    error!(
                        tenant_id = %tenant.id.0,
                        error = %err,
                        "rum aggregation: failed to list sites"
                    );
                    continue;
                }
            };

            for site in sites {
                match self
                    .retention_repository
                    .run_retention_for_site(
                        tenant.id,
                        site.id,
                        RUM_RAW_RETENTION_DAYS,
                        self.batch_size,
                    )
                    .await
                {
                    Ok(outcome) => {
                        sites_processed += 1;
                        if outcome.days_aggregated > 0
                            || outcome.page_views_deleted > 0
                            || outcome.js_errors_deleted > 0
                            || outcome.resource_errors_deleted > 0
                            || outcome.sessions_deleted > 0
                        {
                            info!(
                                tenant_id = %tenant.id.0,
                                site_id = %site.id.0,
                                days_aggregated = outcome.days_aggregated,
                                sessions_deleted = outcome.sessions_deleted,
                                page_views_deleted = outcome.page_views_deleted,
                                js_errors_deleted = outcome.js_errors_deleted,
                                resource_errors_deleted = outcome.resource_errors_deleted,
                                "rum aggregation sweep for site completed"
                            );
                        }
                    }
                    Err(err) => {
                        sites_failed += 1;
                        error!(
                            tenant_id = %tenant.id.0,
                            site_id = %site.id.0,
                            error = %err,
                            "rum aggregation failed for site"
                        );
                    }
                }
            }
        }

        if sites_processed > 0 || sites_failed > 0 {
            info!(
                sites_processed,
                sites_failed, "rum aggregation sweep completed"
            );
        }
        Ok(())
    }

    pub async fn run_forever(&self, cancel: CancellationToken) -> Result<()> {
        info!(
            interval_secs = self.interval.as_secs(),
            "rum aggregation worker started"
        );
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("rum aggregation worker cancellation requested; shutting down");
                    return Ok(());
                }
                _ = ticker.tick() => {
                    if let Err(err) = self.run_once().await {
                        error!(error = %err, "rum aggregation sweep failed");
                    }
                }
            }
        }
    }
}
