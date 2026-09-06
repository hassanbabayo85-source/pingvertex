use async_trait::async_trait;
use domain::SubscriptionPlanChange;
use std::{error::Error, sync::Arc};
use uuid::Uuid;
pub type PlanChangeRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
#[async_trait]
pub trait SubscriptionPlanChangeRepository: Send + Sync {
    async fn create(&self, c: &SubscriptionPlanChange) -> PlanChangeRepoResult<()>;
    async fn update(&self, c: &SubscriptionPlanChange) -> PlanChangeRepoResult<()>;
    async fn find_by_id(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> PlanChangeRepoResult<Option<SubscriptionPlanChange>>;
}
#[async_trait]
impl<T> SubscriptionPlanChangeRepository for Arc<T>
where
    T: SubscriptionPlanChangeRepository + Send + Sync + ?Sized,
{
    async fn create(&self, c: &SubscriptionPlanChange) -> PlanChangeRepoResult<()> {
        (**self).create(c).await
    }
    async fn update(&self, c: &SubscriptionPlanChange) -> PlanChangeRepoResult<()> {
        (**self).update(c).await
    }
    async fn find_by_id(
        &self,
        id: Uuid,
        t: Uuid,
    ) -> PlanChangeRepoResult<Option<SubscriptionPlanChange>> {
        (**self).find_by_id(id, t).await
    }
}
