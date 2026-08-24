use async_trait::async_trait;
use std::sync::Arc;

use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::shared::ids::{ProxyCheckJobId, TenantId, UserId};

use crate::ports::monitor_repository::{QuotaCheckOutcome, RepoResult};

#[async_trait]
pub trait ProxyCheckJobRepository: Send + Sync {
    async fn save_job(&self, job: &ProxyCheckJob, entries: &[ProxyEntry]) -> RepoResult<()>;

    async fn save_job_if_within_quota(
        &self,
        job: &ProxyCheckJob,
        entries: &[ProxyEntry],
        user_id: UserId,
        limit: usize,
    ) -> RepoResult<QuotaCheckOutcome>;

    async fn try_mark_running(&self, id: ProxyCheckJobId) -> RepoResult<bool>;

    async fn update_job(&self, job: &ProxyCheckJob) -> RepoResult<()>;

    async fn find_job_by_id(&self, id: ProxyCheckJobId) -> RepoResult<Option<ProxyCheckJob>>;

    async fn find_jobs_by_user_id(&self, user_id: UserId) -> RepoResult<Vec<ProxyCheckJob>>;

    async fn find_jobs_by_tenant_id(&self, tenant_id: TenantId) -> RepoResult<Vec<ProxyCheckJob>>;

    async fn update_entry(&self, entry: &ProxyEntry) -> RepoResult<()>;

    async fn find_entries_by_job_id(&self, job_id: ProxyCheckJobId) -> RepoResult<Vec<ProxyEntry>>;

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

    async fn save_job_if_within_quota(
        &self,
        job: &ProxyCheckJob,
        entries: &[ProxyEntry],
        user_id: UserId,
        limit: usize,
    ) -> RepoResult<QuotaCheckOutcome> {
        (**self)
            .save_job_if_within_quota(job, entries, user_id, limit)
            .await
    }

    async fn try_mark_running(&self, id: ProxyCheckJobId) -> RepoResult<bool> {
        (**self).try_mark_running(id).await
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
