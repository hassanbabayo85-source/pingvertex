use async_trait::async_trait;
use std::sync::Arc;

use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::shared::ids::{ProxyCheckJobId, TenantId, UserId};

use crate::ports::monitor_repository::RepoResult;

/// Persistence port for `ProxyCheckJob` and its `ProxyEntry` children.
///
/// NOTE: the exact method list here was not fully dictated by the prompt
/// that requested this trait (it named the trait and the pattern to
/// follow — `RepoResult`, `#[async_trait]`, blanket impl on `Arc<T>` —
/// but not the individual methods). The methods below are a judgment
/// call, chosen to mirror `ApiFlowRepository`'s CRUD shape while
/// covering what the domain model needs: a job is created together with
/// its entries in one call (so a job is never visible in storage without
/// its entries), the job's own status/progress fields are updated
/// separately from per-entry results (since the worker checks entries
/// independently and concurrently and reports each result as it
/// finishes), and jobs/entries can be looked up by id or by owner.
#[async_trait]
pub trait ProxyCheckJobRepository: Send + Sync {
    /// Persists a newly created job together with all of its (not yet
    /// checked) entries.
    async fn save_job(&self, job: &ProxyCheckJob, entries: &[ProxyEntry]) -> RepoResult<()>;

    /// Persists an update to the job's own fields (status, progress,
    /// completed_at, ...).
    async fn update_job(&self, job: &ProxyCheckJob) -> RepoResult<()>;

    async fn find_job_by_id(&self, id: ProxyCheckJobId) -> RepoResult<Option<ProxyCheckJob>>;

    async fn find_jobs_by_user_id(&self, user_id: UserId) -> RepoResult<Vec<ProxyCheckJob>>;

    /// Tenant-scoped equivalent of `find_jobs_by_user_id`. Returns every
    /// job belonging to `tenant_id`, regardless of which member of the
    /// tenant submitted it.
    async fn find_jobs_by_tenant_id(&self, _tenant_id: TenantId) -> RepoResult<Vec<ProxyCheckJob>> {
        unimplemented!()
    }

    /// Persists the result of a single checked entry. Called once per
    /// entry by the worker as each proxy finishes being checked.
    async fn update_entry(&self, entry: &ProxyEntry) -> RepoResult<()>;

    async fn find_entries_by_job_id(&self, job_id: ProxyCheckJobId) -> RepoResult<Vec<ProxyEntry>>;

    /// Finds up to `limit` proxy-check jobs with `Pending` status, oldest
    /// first. The worker polls this method to discover new work.
    async fn find_pending_jobs(&self, limit: usize) -> RepoResult<Vec<ProxyCheckJob>>;
}

#[async_trait]
impl<T> ProxyCheckJobRepository for Arc<T>
where
    T: ProxyCheckJobRepository + Send + Sync + ?Sized,
{
    async fn save_job(&self, job: &ProxyCheckJob, entries: &[ProxyEntry]) -> RepoResult<()> {
        (**self).save_job(job, entries).await
    }

    async fn update_job(&self, job: &ProxyCheckJob) -> RepoResult<()> {
        (**self).update_job(job).await
    }

    async fn find_job_by_id(&self, id: ProxyCheckJobId) -> RepoResult<Option<ProxyCheckJob>> {
        (**self).find_job_by_id(id).await
    }

    async fn find_jobs_by_user_id(&self, user_id: UserId) -> RepoResult<Vec<ProxyCheckJob>> {
        (**self).find_jobs_by_user_id(user_id).await
    }

    async fn find_jobs_by_tenant_id(&self, tenant_id: TenantId) -> RepoResult<Vec<ProxyCheckJob>> {
        (**self).find_jobs_by_tenant_id(tenant_id).await
    }

    async fn update_entry(&self, entry: &ProxyEntry) -> RepoResult<()> {
        (**self).update_entry(entry).await
    }

    async fn find_entries_by_job_id(&self, job_id: ProxyCheckJobId) -> RepoResult<Vec<ProxyEntry>> {
        (**self).find_entries_by_job_id(job_id).await
    }

    async fn find_pending_jobs(&self, limit: usize) -> RepoResult<Vec<ProxyCheckJob>> {
        (**self).find_pending_jobs(limit).await
    }
}
