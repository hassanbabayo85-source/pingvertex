use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dto::{
    BulkMonitorImportJob, ClaimedBulkMonitorImportRow, CreateBulkMonitorImportJob,
    CreateBulkMonitorImportJobOutcome,
};

pub type BulkMonitorImportRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait BulkMonitorImportRepository: Send + Sync {
    async fn create_confirmed_job(
        &self,
        input: &CreateBulkMonitorImportJob,
    ) -> BulkMonitorImportRepoResult<CreateBulkMonitorImportJobOutcome>;

    async fn find_job_for_tenant(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorImportRepoResult<Option<BulkMonitorImportJob>>;

    async fn find_job_by_idempotency_key(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
    ) -> BulkMonitorImportRepoResult<Option<(BulkMonitorImportJob, Option<String>, i32)>>;

    async fn claim_jobs(
        &self,
        worker_id: Uuid,
        limit: u32,
        lease_until: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<Vec<BulkMonitorImportJob>>;

    async fn renew_job_lease(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        lease_until: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<bool>;

    async fn claim_rows(
        &self,
        job_id: Uuid,
        limit: u32,
        stale_before: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<Vec<ClaimedBulkMonitorImportRow>>;

    async fn mark_row_created(
        &self,
        row_id: i64,
        monitor_id: Uuid,
    ) -> BulkMonitorImportRepoResult<()>;

    async fn retry_row(
        &self,
        row_id: i64,
        error: &str,
        next_attempt_at: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<()>;

    async fn fail_row(&self, row_id: i64, error: &str) -> BulkMonitorImportRepoResult<()>;

    async fn skip_row(&self, row_id: i64, reason: &str) -> BulkMonitorImportRepoResult<()>;

    async fn pause_job(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorImportRepoResult<bool>;

    async fn resume_job(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorImportRepoResult<bool>;

    async fn refresh_job_state(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
    ) -> BulkMonitorImportRepoResult<BulkMonitorImportJob>;

    async fn release_job_after_error(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        error: &str,
    ) -> BulkMonitorImportRepoResult<()>;
}

#[async_trait]
impl<T> BulkMonitorImportRepository for Arc<T>
where
    T: BulkMonitorImportRepository + Send + Sync + ?Sized,
{
    async fn create_confirmed_job(
        &self,
        input: &CreateBulkMonitorImportJob,
    ) -> BulkMonitorImportRepoResult<CreateBulkMonitorImportJobOutcome> {
        (**self).create_confirmed_job(input).await
    }

    async fn find_job_for_tenant(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorImportRepoResult<Option<BulkMonitorImportJob>> {
        (**self).find_job_for_tenant(job_id, tenant_id).await
    }

    async fn find_job_by_idempotency_key(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
    ) -> BulkMonitorImportRepoResult<Option<(BulkMonitorImportJob, Option<String>, i32)>> {
        (**self)
            .find_job_by_idempotency_key(tenant_id, idempotency_key)
            .await
    }

    async fn claim_jobs(
        &self,
        worker_id: Uuid,
        limit: u32,
        lease_until: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<Vec<BulkMonitorImportJob>> {
        (**self).claim_jobs(worker_id, limit, lease_until).await
    }

    async fn renew_job_lease(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        lease_until: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<bool> {
        (**self)
            .renew_job_lease(job_id, worker_id, lease_until)
            .await
    }

    async fn claim_rows(
        &self,
        job_id: Uuid,
        limit: u32,
        stale_before: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<Vec<ClaimedBulkMonitorImportRow>> {
        (**self).claim_rows(job_id, limit, stale_before).await
    }

    async fn mark_row_created(
        &self,
        row_id: i64,
        monitor_id: Uuid,
    ) -> BulkMonitorImportRepoResult<()> {
        (**self).mark_row_created(row_id, monitor_id).await
    }

    async fn retry_row(
        &self,
        row_id: i64,
        error: &str,
        next_attempt_at: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<()> {
        (**self).retry_row(row_id, error, next_attempt_at).await
    }

    async fn fail_row(&self, row_id: i64, error: &str) -> BulkMonitorImportRepoResult<()> {
        (**self).fail_row(row_id, error).await
    }

    async fn skip_row(&self, row_id: i64, reason: &str) -> BulkMonitorImportRepoResult<()> {
        (**self).skip_row(row_id, reason).await
    }

    async fn pause_job(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorImportRepoResult<bool> {
        (**self).pause_job(job_id, tenant_id).await
    }

    async fn resume_job(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorImportRepoResult<bool> {
        (**self).resume_job(job_id, tenant_id).await
    }

    async fn refresh_job_state(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
    ) -> BulkMonitorImportRepoResult<BulkMonitorImportJob> {
        (**self).refresh_job_state(job_id, worker_id).await
    }

    async fn release_job_after_error(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        error: &str,
    ) -> BulkMonitorImportRepoResult<()> {
        (**self)
            .release_job_after_error(job_id, worker_id, error)
            .await
    }
}
