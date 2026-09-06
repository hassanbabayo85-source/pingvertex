use async_trait::async_trait;
use domain::UsageEvent;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub type UsageEventRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait UsageEventRepository: Send + Sync {
    async fn find_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> UsageEventRepoResult<Option<UsageEvent>>;

    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent>;
}

#[async_trait]
impl<T> UsageEventRepository for Arc<T>
where
    T: UsageEventRepository + Send + Sync + ?Sized,
{
    async fn find_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> UsageEventRepoResult<Option<UsageEvent>> {
        (**self).find_by_request(tenant_id, request_id).await
    }

    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent> {
        (**self).create(event).await
    }
}
