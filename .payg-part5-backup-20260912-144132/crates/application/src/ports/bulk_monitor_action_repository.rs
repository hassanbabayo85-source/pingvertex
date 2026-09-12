use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::{error::Error, sync::Arc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkMonitorActionJob {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub actor_user_id: Uuid,
    pub action: String,
    pub monitor_ids: Option<Vec<Uuid>>,
    pub all: bool,
    pub reason: Option<String>,
    pub idempotency_key: Option<String>,
    pub status: String,
    pub affected: Option<u64>,
    pub error: Option<String>,
}

pub type BulkMonitorActionRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait BulkMonitorActionRepository: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    async fn create(
        &self,
        tenant_id: Uuid,
        actor_user_id: Uuid,
        action: &str,
        monitor_ids: Option<&[Uuid]>,
        all: bool,
        reason: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> BulkMonitorActionRepoResult<BulkMonitorActionJob>;

    async fn find_for_tenant(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorActionRepoResult<Option<BulkMonitorActionJob>>;

    async fn claim_jobs(
        &self,
        worker_id: Uuid,
        limit: u32,
        lease_until: DateTime<Utc>,
    ) -> BulkMonitorActionRepoResult<Vec<BulkMonitorActionJob>>;

    async fn complete(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        affected: u64,
    ) -> BulkMonitorActionRepoResult<bool>;

    async fn fail(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        error: &str,
    ) -> BulkMonitorActionRepoResult<bool>;
}

#[async_trait]
impl<T> BulkMonitorActionRepository for Arc<T>
where
    T: BulkMonitorActionRepository + Send + Sync + ?Sized,
{
    #[allow(clippy::too_many_arguments)]
    async fn create(
        &self,
        tenant_id: Uuid,
        actor_user_id: Uuid,
        action: &str,
        monitor_ids: Option<&[Uuid]>,
        all: bool,
        reason: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> BulkMonitorActionRepoResult<BulkMonitorActionJob> {
        (**self)
            .create(
                tenant_id,
                actor_user_id,
                action,
                monitor_ids,
                all,
                reason,
                idempotency_key,
            )
            .await
    }
    async fn find_for_tenant(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorActionRepoResult<Option<BulkMonitorActionJob>> {
        (**self).find_for_tenant(job_id, tenant_id).await
    }
    async fn claim_jobs(
        &self,
        worker_id: Uuid,
        limit: u32,
        lease_until: DateTime<Utc>,
    ) -> BulkMonitorActionRepoResult<Vec<BulkMonitorActionJob>> {
        (**self).claim_jobs(worker_id, limit, lease_until).await
    }
    async fn complete(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        affected: u64,
    ) -> BulkMonitorActionRepoResult<bool> {
        (**self).complete(job_id, worker_id, affected).await
    }
    async fn fail(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        error: &str,
    ) -> BulkMonitorActionRepoResult<bool> {
        (**self).fail(job_id, worker_id, error).await
    }
}
