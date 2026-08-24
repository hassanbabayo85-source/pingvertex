use std::{error::Error, sync::Arc, time::Duration};

use application::ports::ProxyCheckJobRepository;
use tracing::error;

use crate::proxy_checker::run_proxy_check_job;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

const PROXY_CHECK_POLL_INTERVAL_SECS: u64 = 5;
const MAX_JOBS_PER_SWEEP: usize = 10;

#[derive(Clone)]
pub struct ProxyCheckWorker {
    repository: Arc<dyn ProxyCheckJobRepository>,
}

impl ProxyCheckWorker {
    pub fn new(repository: Arc<dyn ProxyCheckJobRepository>) -> Self {
        Self { repository }
    }

    pub async fn run_once(&self) -> Result<()> {
        let jobs = self
            .repository
            .find_pending_jobs(MAX_JOBS_PER_SWEEP)
            .await?;

        for job in jobs {
            let job_id = job.id();

            let claimed = self.repository.try_mark_running(job_id).await?;
            if !claimed {
                continue;
            }

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
