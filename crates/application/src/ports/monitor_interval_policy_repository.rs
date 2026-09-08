use async_trait::async_trait;
use domain::MonitorKind;
use std::{error::Error, sync::Arc};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Server-controlled PAYG execution-frequency policy.
#[async_trait]
pub trait MonitorIntervalPolicyRepository: Send + Sync {
    async fn minimum_interval_seconds(&self, kind: MonitorKind) -> RepoResult<u32>;
}

#[async_trait]
impl<T> MonitorIntervalPolicyRepository for Arc<T>
where
    T: MonitorIntervalPolicyRepository + Send + Sync + ?Sized,
{
    async fn minimum_interval_seconds(&self, kind: MonitorKind) -> RepoResult<u32> {
        (**self).minimum_interval_seconds(kind).await
    }
}

