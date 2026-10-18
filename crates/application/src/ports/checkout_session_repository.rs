use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::CheckoutSession;
use domain::shared::ids::CheckoutSessionId;

pub type CheckoutRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for checkout sessions.
///
/// Only `create`, `find_by_id`, and `update` are needed. The
/// verification endpoint receives the internal checkout session id, not
/// the provider's checkout id, so no provider-id lookup is required.
#[async_trait]
pub trait CheckoutSessionRepository: Send + Sync {
    async fn create(&self, session: &CheckoutSession) -> CheckoutRepoResult<()>;

    async fn find_by_id(
        &self,
        id: CheckoutSessionId,
    ) -> CheckoutRepoResult<Option<CheckoutSession>>;

    async fn update(&self, session: &CheckoutSession) -> CheckoutRepoResult<()>;
}

#[async_trait]
impl<T> CheckoutSessionRepository for Arc<T>
where
    T: CheckoutSessionRepository + Send + Sync + ?Sized,
{
    async fn create(&self, session: &CheckoutSession) -> CheckoutRepoResult<()> {
        (**self).create(session).await
    }

    async fn find_by_id(
        &self,
        id: CheckoutSessionId,
    ) -> CheckoutRepoResult<Option<CheckoutSession>> {
        (**self).find_by_id(id).await
    }

    async fn update(&self, session: &CheckoutSession) -> CheckoutRepoResult<()> {
        (**self).update(session).await
    }
}
