use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use application::ports::monitor_repository::QuotaCheckOutcome;
use application::ports::{ProxyCheckJobRepository, ProxyResultStatusFilter};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::enums::{ProxyCheckJobStatus, ProxyScheme};
use domain::shared::ids::{ProxyCheckJobId, ProxyEntryId, TenantId, UserId};
use domain::value_objects::{ProxyCredentials, ProxyHost, ProxyResult};

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone)]
struct StoredProxyEntry {
    job_id: ProxyCheckJobId,
    host: ProxyHost,
    port: u16,
    scheme: ProxyScheme,
    credentials: Option<(String, String)>,
    result: Option<ProxyResult>,
}

#[derive(Clone)]
pub struct InMemoryProxyCheckJobRepository {
    jobs: Arc<RwLock<HashMap<ProxyCheckJobId, ProxyCheckJob>>>,
    entries: Arc<RwLock<HashMap<ProxyEntryId, StoredProxyEntry>>>,
    quota_lock: Arc<tokio::sync::Mutex<()>>,
}
impl InMemoryProxyCheckJobRepository {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            entries: Arc::new(RwLock::new(HashMap::new())),
            quota_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}
impl Default for InMemoryProxyCheckJobRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProxyCheckJobRepository for InMemoryProxyCheckJobRepository {
    async fn save_job(&self, job: &ProxyCheckJob, entries: &[ProxyEntry]) -> RepoResult<()> {
        self.jobs.write().unwrap().insert(job.id(), job.clone());
        let mut stored = self.entries.write().unwrap();
        for entry in entries {
            let credentials = entry
                .credentials()
                .map(|c| (c.username().to_string(), c.password().to_string()));
            stored.insert(
                entry.id(),
                StoredProxyEntry {
                    job_id: entry.job_id(),
                    host: entry.host().clone(),
                    port: entry.port(),
                    scheme: entry.scheme().clone(),
                    credentials,
                    result: entry.result().cloned(),
                },
            );
        }
        Ok(())
    }

    async fn save_job_if_within_quota(
        &self,
        job: &ProxyCheckJob,
        entries: &[ProxyEntry],
        user_id: UserId,
        tenant_id: TenantId,
        limit: usize,
    ) -> RepoResult<QuotaCheckOutcome> {
        if job.user_id() != user_id || job.tenant_id() != tenant_id {
            return Err("proxy job ownership mismatch".into());
        }
        let _guard = self.quota_lock.lock().await;
        let today = Utc::now().date_naive();
        let count = self
            .jobs
            .read()
            .unwrap()
            .values()
            .filter(|j| {
                j.user_id() == user_id
                    && j.tenant_id() == tenant_id
                    && j.created_at().date_naive() == today
            })
            .count();
        if count >= limit {
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }
        self.save_job(job, entries).await?;
        Ok(QuotaCheckOutcome::Saved)
    }

    async fn try_mark_running(&self, id: ProxyCheckJobId, worker_id: &str) -> RepoResult<bool> {
        let mut jobs = self.jobs.write().unwrap();
        let Some(job) = jobs.get_mut(&id) else {
            return Ok(false);
        };
        if job.status() != ProxyCheckJobStatus::Pending {
            return Ok(false);
        }
        job.set_worker_lease(worker_id.to_string(), Utc::now());
        Ok(true)
    }

    async fn heartbeat(&self, id: ProxyCheckJobId, worker_id: &str) -> RepoResult<bool> {
        let mut jobs = self.jobs.write().unwrap();
        let Some(job) = jobs.get_mut(&id) else {
            return Ok(false);
        };
        if job.status() != ProxyCheckJobStatus::Running || job.worker_id() != Some(worker_id) {
            return Ok(false);
        }
        job.set_worker_lease(worker_id.to_string(), Utc::now());
        Ok(true)
    }

    async fn recover_stale_jobs(
        &self,
        stale_before: DateTime<Utc>,
        max_retries: u32,
    ) -> RepoResult<u64> {
        let mut jobs = self.jobs.write().unwrap();
        let mut count = 0;
        for job in jobs.values_mut() {
            if job.status() == ProxyCheckJobStatus::Running
                && job.heartbeat_at().is_some_and(|t| t < stale_before)
            {
                // Reconstructing through the public domain API is intentionally
                // avoided here; stale recovery is a persistence concern.
                if job.retry_count() + 1 >= max_retries {
                    let _ = job.mark_failed("stale proxy check job exceeded retry limit");
                } else {
                    // A fresh pending job must have no lease. We cannot mutate
                    // status directly, so use the same persisted constructor.
                    let replacement = ProxyCheckJob::from_persisted(
                        job.id(),
                        job.user_id(),
                        job.tenant_id(),
                        ProxyCheckJobStatus::Pending,
                        job.created_at(),
                        job.started_at(),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        job.retry_count() + 1,
                        job.total_count(),
                        job.checked_count(),
                        job.successful_count(),
                        job.failed_count(),
                    );
                    *job = replacement;
                }
                count += 1;
            }
        }
        Ok(count)
    }

    async fn update_job(&self, job: &ProxyCheckJob) -> RepoResult<()> {
        let mut jobs = self.jobs.write().unwrap();
        if let Some(previous) = jobs.get(&job.id()) {
            let valid = matches!(
                (previous.status(), job.status()),
                (ProxyCheckJobStatus::Pending, ProxyCheckJobStatus::Running)
                    | (ProxyCheckJobStatus::Running, ProxyCheckJobStatus::Running)
                    | (ProxyCheckJobStatus::Running, ProxyCheckJobStatus::Completed)
                    | (ProxyCheckJobStatus::Running, ProxyCheckJobStatus::Failed)
                    | (ProxyCheckJobStatus::Running, ProxyCheckJobStatus::Canceled)
                    | (ProxyCheckJobStatus::Pending, ProxyCheckJobStatus::Pending)
                    | (ProxyCheckJobStatus::Pending, ProxyCheckJobStatus::Canceled)
                    | (
                        ProxyCheckJobStatus::Completed,
                        ProxyCheckJobStatus::Completed
                    )
                    | (ProxyCheckJobStatus::Failed, ProxyCheckJobStatus::Failed)
                    | (ProxyCheckJobStatus::Canceled, ProxyCheckJobStatus::Canceled)
            );
            if !valid {
                return Err(format!(
                    "invalid proxy check job status transition: {:?} -> {:?}",
                    previous.status(),
                    job.status()
                )
                .into());
            }
            if previous.status() == ProxyCheckJobStatus::Canceled
                && job.status() != ProxyCheckJobStatus::Canceled
            {
                return Err("canceled proxy job cannot be restarted".into());
            }
        }
        jobs.insert(job.id(), job.clone());
        Ok(())
    }

    async fn find_job_by_id(
        &self,
        id: ProxyCheckJobId,
        tenant_id: TenantId,
    ) -> RepoResult<Option<ProxyCheckJob>> {
        Ok(self
            .jobs
            .read()
            .unwrap()
            .get(&id)
            .filter(|j| j.tenant_id() == tenant_id)
            .cloned())
    }

    async fn find_jobs_by_user_id(
        &self,
        user_id: UserId,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<ProxyCheckJob>> {
        Ok(self
            .jobs
            .read()
            .unwrap()
            .values()
            .filter(|j| j.user_id() == user_id && j.tenant_id() == tenant_id)
            .cloned()
            .collect())
    }

    async fn find_jobs_by_tenant_id(&self, tenant_id: TenantId) -> RepoResult<Vec<ProxyCheckJob>> {
        Ok(self
            .jobs
            .read()
            .unwrap()
            .values()
            .filter(|j| j.tenant_id() == tenant_id)
            .cloned()
            .collect())
    }

    async fn update_entry(&self, entry: &ProxyEntry, tenant_id: TenantId) -> RepoResult<()> {
        let jobs = self.jobs.read().unwrap();
        let Some(job) = jobs.get(&entry.job_id()) else {
            return Err("proxy job not found".into());
        };
        if job.tenant_id() != tenant_id {
            return Err("proxy entry tenant mismatch".into());
        }
        drop(jobs);
        let mut entries = self.entries.write().unwrap();
        let Some(stored) = entries.get_mut(&entry.id()) else {
            return Err("proxy entry not found".into());
        };
        stored.result = entry.result().cloned();
        Ok(())
    }

    async fn find_entries_by_job_id(
        &self,
        job_id: ProxyCheckJobId,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<ProxyEntry>> {
        let jobs = self.jobs.read().unwrap();
        if !matches!(jobs.get(&job_id), Some(j) if j.tenant_id() == tenant_id) {
            return Ok(Vec::new());
        }
        let stored = self.entries.read().unwrap();
        stored
            .iter()
            .filter(|(_, e)| e.job_id == job_id)
            .map(|(id, e)| {
                let credentials = match &e.credentials {
                    Some((u, p)) => Some(ProxyCredentials::new(u.clone(), p.clone())?),
                    None => None,
                };
                Ok(ProxyEntry::from_persisted(
                    *id,
                    e.job_id,
                    e.host.clone(),
                    e.port,
                    e.scheme.clone(),
                    credentials,
                    e.result.clone(),
                ))
            })
            .collect()
    }

    async fn list_entries(
        &self,
        job_id: ProxyCheckJobId,
        tenant_id: TenantId,
        offset: u32,
        limit: u32,
        status: Option<ProxyResultStatusFilter>,
        scheme: Option<ProxyScheme>,
        working_only: bool,
    ) -> RepoResult<(Vec<ProxyEntry>, u32)> {
        let all = self.find_entries_by_job_id(job_id, tenant_id).await?;
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|e| {
                if let Some(ref s) = scheme
                    && e.scheme() != s
                {
                    return false;
                }
                let working = e.result().is_some_and(|r| r.reachable);
                if working_only && !working {
                    return false;
                }
                match status {
                    Some(ProxyResultStatusFilter::Working) => working,
                    Some(ProxyResultStatusFilter::Failed) => {
                        e.result().is_some_and(|r| !r.reachable)
                    }
                    None => true,
                }
            })
            .collect();
        let total = filtered.len() as u32;
        let data = filtered
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        Ok((data, total))
    }

    async fn cancel_job(&self, id: ProxyCheckJobId, tenant_id: TenantId) -> RepoResult<bool> {
        let mut jobs = self.jobs.write().unwrap();
        let Some(job) = jobs.get_mut(&id) else {
            return Ok(false);
        };
        if job.tenant_id() != tenant_id
            || !matches!(
                job.status(),
                ProxyCheckJobStatus::Pending | ProxyCheckJobStatus::Running
            )
        {
            return Ok(false);
        }
        Ok(job.mark_canceled())
    }

    async fn find_pending_jobs(&self, limit: usize) -> RepoResult<Vec<ProxyCheckJob>> {
        Ok(self
            .jobs
            .read()
            .unwrap()
            .values()
            .filter(|j| j.status() == ProxyCheckJobStatus::Pending)
            .take(limit)
            .cloned()
            .collect())
    }
}
