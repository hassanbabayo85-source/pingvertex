use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use application::ports::ProxyCheckJobRepository;
use application::ports::monitor_repository::QuotaCheckOutcome;
use async_trait::async_trait;
use chrono::Utc;
use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::enums::{ProxyCheckJobStatus, ProxyScheme};
use domain::shared::ids::{ProxyCheckJobId, ProxyEntryId, TenantId, UserId};
use domain::value_objects::{ProxyCredentials, ProxyHost, ProxyResult};

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone)]
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
        limit: usize,
    ) -> RepoResult<QuotaCheckOutcome> {
        let _guard = self.quota_lock.lock().await;

        let today = Utc::now().date_naive();
        let count = self
            .jobs
            .read()
            .unwrap()
            .values()
            .filter(|j| j.user_id() == user_id && j.created_at().date_naive() == today)
            .count();

        if count >= limit {
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }

        self.save_job(job, entries).await?;
        Ok(QuotaCheckOutcome::Saved)
    }

    async fn try_mark_running(&self, id: ProxyCheckJobId) -> RepoResult<bool> {
        let mut jobs = self.jobs.write().unwrap();
        let Some(job) = jobs.get_mut(&id) else {
            return Ok(false);
        };

        if job.status() != ProxyCheckJobStatus::Pending {
            return Ok(false);
        }

        job.mark_running();
        Ok(true)
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
                    | (ProxyCheckJobStatus::Pending, ProxyCheckJobStatus::Pending)
                    | (
                        ProxyCheckJobStatus::Completed,
                        ProxyCheckJobStatus::Completed
                    )
                    | (ProxyCheckJobStatus::Failed, ProxyCheckJobStatus::Failed)
            );
            if !valid {
                return Err(format!(
                    "invalid proxy check job status transition: {:?} -> {:?}",
                    previous.status(),
                    job.status()
                )
                .into());
            }
        }
        jobs.insert(job.id(), job.clone());
        Ok(())
    }

    async fn find_job_by_id(&self, id: ProxyCheckJobId) -> RepoResult<Option<ProxyCheckJob>> {
        Ok(self.jobs.read().unwrap().get(&id).cloned())
    }

    async fn find_jobs_by_user_id(&self, user_id: UserId) -> RepoResult<Vec<ProxyCheckJob>> {
        Ok(self
            .jobs
            .read()
            .unwrap()
            .values()
            .filter(|job| job.user_id() == user_id)
            .cloned()
            .collect())
    }

    async fn find_jobs_by_tenant_id(&self, tenant_id: TenantId) -> RepoResult<Vec<ProxyCheckJob>> {
        Ok(self
            .jobs
            .read()
            .unwrap()
            .values()
            .filter(|job| job.tenant_id() == tenant_id)
            .cloned()
            .collect())
    }

    async fn update_entry(&self, entry: &ProxyEntry) -> RepoResult<()> {
        if let Some(stored) = self.entries.write().unwrap().get_mut(&entry.id()) {
            stored.result = entry.result().cloned();
        }
        Ok(())
    }

    async fn find_entries_by_job_id(&self, job_id: ProxyCheckJobId) -> RepoResult<Vec<ProxyEntry>> {
        let stored = self.entries.read().unwrap();

        stored
            .iter()
            .filter(|(_, entry)| entry.job_id == job_id)
            .map(|(id, entry)| {
                let credentials = match &entry.credentials {
                    Some((username, password)) => {
                        Some(ProxyCredentials::new(username.clone(), password.clone())?)
                    }
                    None => None,
                };

                Ok(ProxyEntry::from_persisted(
                    *id,
                    entry.job_id,
                    entry.host.clone(),
                    entry.port,
                    entry.scheme.clone(),
                    credentials,
                    entry.result.clone(),
                ))
            })
            .collect()
    }

    async fn find_pending_jobs(&self, limit: usize) -> RepoResult<Vec<ProxyCheckJob>> {
        Ok(self
            .jobs
            .read()
            .unwrap()
            .values()
            .filter(|job| job.status() == ProxyCheckJobStatus::Pending)
            .take(limit)
            .cloned()
            .collect())
    }
}
