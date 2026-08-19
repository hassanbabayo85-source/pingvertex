use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use application::ports::ProxyCheckJobRepository;
use async_trait::async_trait;
use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::enums::{ProxyCheckJobStatus, ProxyScheme};
use domain::shared::ids::{ProxyCheckJobId, ProxyEntryId, TenantId, UserId};
use domain::value_objects::{ProxyCredentials, ProxyHost, ProxyResult};

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A `ProxyEntry`'s persisted components, stored separately from the
/// domain type itself.
///
/// Mirrors `PostgresProxyCheckJobRepository`'s `ProxyEntryRow`: `ProxyEntry`
/// does not derive `Clone` (nor does `ProxyCredentials`, deliberately, per
/// its own security doc comment), so this repository — like the Postgres
/// one — stores the raw components instead and reconstructs a `ProxyEntry`
/// via `ProxyEntry::from_persisted` on every read. `credentials` is kept
/// as a plain `(username, password)` tuple (this repository's in-memory
/// storage has no encryption boundary to mirror; `PostgresProxyCheckJobRepository`
/// stores ciphertext instead) and re-validated through
/// `ProxyCredentials::new` when reconstructing — this cannot fail, since
/// the same values already passed validation once when the entry was
/// first saved.
#[derive(Debug, Clone)]
struct StoredProxyEntry {
    job_id: ProxyCheckJobId,
    host: ProxyHost,
    port: u16,
    scheme: ProxyScheme,
    credentials: Option<(String, String)>,
    result: Option<ProxyResult>,
}

/// In-memory `ProxyCheckJobRepository`, for tests only (mirrors
/// `InMemoryApiFlowRepository`'s role and shape).
#[derive(Debug, Clone, Default)]
pub struct InMemoryProxyCheckJobRepository {
    jobs: Arc<RwLock<HashMap<ProxyCheckJobId, ProxyCheckJob>>>,
    entries: Arc<RwLock<HashMap<ProxyEntryId, StoredProxyEntry>>>,
}

impl InMemoryProxyCheckJobRepository {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
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

    async fn update_job(&self, job: &ProxyCheckJob) -> RepoResult<()> {
        self.jobs.write().unwrap().insert(job.id(), job.clone());
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
