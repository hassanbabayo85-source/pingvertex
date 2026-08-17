use std::{error::Error, sync::Arc, time::Duration};

use application::ports::ProxyCheckJobRepository;
use tracing::error;

use crate::proxy_checker::run_proxy_check_job;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// How long the worker waits between polls for new `Pending` proxy-check
/// jobs. Five seconds is intentionally short so submitted jobs begin
/// checking quickly, while still avoiding a hot spin loop.
const PROXY_CHECK_POLL_INTERVAL_SECS: u64 = 5;

/// Maximum number of jobs pulled from the `Pending` queue per sweep.
/// Keeps a single sweep from monopolizing the worker when many jobs are
/// pending at once; the rest will be picked up on subsequent sweeps.
const MAX_JOBS_PER_SWEEP: usize = 10;

/// Polls for `Pending` proxy-check jobs and runs them one job at a time.
///
/// Dispatch is intentionally simple: a periodic poll against the
/// `find_pending_jobs` repository method. LISTEN/NOTIFY or a durable
/// queue could replace this later without touching the actual checking
/// logic in `proxy_checker::run_proxy_check_job`.
#[derive(Clone)]
pub struct ProxyCheckWorker {
    repository: Arc<dyn ProxyCheckJobRepository>,
}

impl ProxyCheckWorker {
    pub fn new(repository: Arc<dyn ProxyCheckJobRepository>) -> Self {
        Self { repository }
    }

    /// Runs a single poll-and-dispatch sweep.
    ///
    /// Errors loading a job's entries are logged and skipped so one bad
    /// job cannot poison the whole sweep. Errors *during* a job's check
    /// are handled by `run_proxy_check_job` itself; the worker only
    /// logs them here.
    pub async fn run_once(&self) -> Result<()> {
        let jobs = self
            .repository
            .find_pending_jobs(MAX_JOBS_PER_SWEEP)
            .await?;

        for job in jobs {
            let job_id = job.id();

            match self.repository.find_entries_by_job_id(job_id).await {
                Ok(entries) => {
                    if let Err(err) = run_proxy_check_job(&*self.repository, job, entries).await {
                        error!(
                            job_id = %job_id,
                            error = %err,
                            "proxy check job failed during execution"
                        );
                    }
                }
                Err(err) => {
                    error!(
                        job_id = %job_id,
                        error = %err,
                        "failed to load proxy entries for job; skipping"
                    );
                }
            }
        }

        Ok(())
    }

    /// Runs forever, polling every `PROXY_CHECK_POLL_INTERVAL_SECS`.
    pub async fn run_forever(&self) -> Result<()> {
        let mut interval =
            tokio::time::interval(Duration::from_secs(PROXY_CHECK_POLL_INTERVAL_SECS));

        loop {
            interval.tick().await;

            if let Err(err) = self.run_once().await {
                error!(error = %err, "proxy check worker sweep failed");
            }
        }
    }
}
