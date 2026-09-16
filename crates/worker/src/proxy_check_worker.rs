use std::{error::Error, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

use crate::proxy_checker::run_proxy_check_job;
use application::MeterUsageUseCase;
use application::ports::ProxyCheckJobRepository;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

const PROXY_CHECK_POLL_INTERVAL_SECS: u64 = 5;
const MAX_JOBS_PER_SWEEP: usize = 10;
const PROXY_JOB_LEASE_SECS: i64 = 60;
const PROXY_JOB_MAX_RETRIES: u32 = 3;

#[derive(Clone)]
pub struct ProxyCheckWorker {
    repository: Arc<dyn ProxyCheckJobRepository>,
    metering: Arc<MeterUsageUseCase>,
    worker_id: String,
}
impl ProxyCheckWorker {
    pub fn new(
        repository: Arc<dyn ProxyCheckJobRepository>,
        metering: Arc<MeterUsageUseCase>,
    ) -> Self {
        Self {
            repository,
            metering,
            worker_id: Uuid::new_v4().to_string(),
        }
    }

    pub async fn run_once(&self) -> Result<()> {
        let stale_before = chrono::Utc::now() - chrono::Duration::seconds(PROXY_JOB_LEASE_SECS);
        if let Err(err) = self
            .repository
            .recover_stale_jobs(stale_before, PROXY_JOB_MAX_RETRIES)
            .await
        {
            error!(error=%err, "proxy stale-job recovery failed");
        }

        let jobs = self
            .repository
            .find_pending_jobs(MAX_JOBS_PER_SWEEP)
            .await?;
        for candidate in jobs {
            let job_id = candidate.id();

            // Atomic claim is the single source of truth for ownership.
            // No executor-side mark_running is allowed.
            if !self
                .repository
                .try_mark_running(job_id, &self.worker_id)
                .await?
            {
                continue;
            }

            let tenant_id = candidate.tenant_id();
            let running_job = match self.repository.find_job_by_id(job_id, tenant_id).await {
                Ok(Some(job)) if job.status() == domain::ProxyCheckJobStatus::Running => job,
                Ok(Some(_)) => continue,
                Ok(None) => {
                    error!(job_id=%job_id, "claimed proxy check job disappeared");
                    continue;
                }
                Err(err) => {
                    error!(job_id=%job_id, error=%err, "failed to reload claimed proxy check job");
                    let mut failed = candidate.clone();
                    failed.set_worker_lease(self.worker_id.clone(), chrono::Utc::now());
                    if failed.mark_failed("failed to reload claimed proxy check job") {
                        let _ = self.repository.update_job(&failed).await;
                    }
                    continue;
                }
            };

            let entries = match self
                .repository
                .find_entries_by_job_id(job_id, tenant_id)
                .await
            {
                Ok(entries) => entries,
                Err(err) => {
                    let mut failed = running_job;
                    if failed.mark_failed(format!("failed to load proxy entries: {err}"))
                        && let Err(update_err) = self.repository.update_job(&failed).await
                    {
                        error!(
                            job_id=%job_id,
                            error=%update_err,
                            "failed to persist entry-load failure"
                        );
                    }
                    continue;
                }
            };

            // A zero-entry job and a malformed persisted job do not perform a
            // network attempt, so they must never consume PAYG credit.
            if entries.is_empty() || entries.len() != running_job.total_count() as usize {
                if let Err(err) = run_proxy_check_job(
                    &*self.repository,
                    running_job,
                    entries,
                    &self.worker_id,
                    None,
                )
                .await
                {
                    error!(job_id=%job_id, error=%err, "proxy preflight failure");
                }
                continue;
            }

            // Per-entry billing now lives inside run_proxy_check_job, where the
            // durable job + entry identity is available. This prevents a bulk
            // proxy job from being charged once for many entries and keeps
            // retries idempotent per logical proxy attempt.
            //
            // Escape hatch: when PROXY_CHECK_BILLING_DISABLED=true we hand
            // `None` to the executor, which makes it run every entry
            // through check_single_entry without touching the PAYG layer.
            // This is intended for development / diagnostics only — a
            // production deployment must never set it.
            let billing_disabled =
                std::env::var("PROXY_CHECK_BILLING_DISABLED").as_deref() == Ok("true");

            if billing_disabled {
                tracing::warn!(
                    job_id = %job_id,
                    "PROXY_CHECK_BILLING_DISABLED=true — running proxy checks WITHOUT PAYG metering"
                );
            }

            let metering_opt = if billing_disabled {
                None
            } else {
                Some(self.metering.clone())
            };

            let result = run_proxy_check_job(
                &*self.repository,
                running_job,
                entries,
                &self.worker_id,
                metering_opt,
            )
            .await;

            if let Err(err) = result {
                let reason = format!("proxy billing/execution failed: {err}");
                error!(job_id=%job_id, error=%err, "proxy check executor failed");
                // The executor normally terminalizes the job itself. This
                // fallback prevents an unexpected propagated error from
                // leaving a claimed job in running forever and preserves a
                // safe, user-visible billing reason.
                if let Ok(Some(mut latest)) =
                    self.repository.find_job_by_id(job_id, tenant_id).await
                    && latest.status() == domain::ProxyCheckJobStatus::Running
                    && latest.mark_failed(reason)
                    && let Err(update_err) = self.repository.update_job(&latest).await
                {
                    error!(
                        job_id=%job_id,
                        error=%update_err,
                        "failed to persist proxy worker failure"
                    );
                }
            }
        }
        Ok(())
    }

    pub async fn run_forever(&self, token: CancellationToken) -> Result<()> {
        let mut interval =
            tokio::time::interval(Duration::from_secs(PROXY_CHECK_POLL_INTERVAL_SECS));
        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("proxy check worker cancellation requested; shutting down");
                    return Ok(());
                }
                _ = interval.tick() => {
                    if let Err(err) = self.run_once().await {
                        error!(error=%err, "proxy check worker sweep failed");
                    }
                }
            }
        }
    }
}
