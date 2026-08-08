use std::error::Error;

use async_trait::async_trait;
use domain::UserId;

type ServiceResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait TokenService: Send + Sync {
    async fn generate(&self, user_id: UserId) -> ServiceResult<String>;

    async fn verify(&self, token: &str) -> ServiceResult<UserId>;
}

use std::sync::Arc;

#[async_trait]
impl<T> TokenService for Arc<T>
where
    T: TokenService + ?Sized,
{
    async fn generate(&self, user_id: domain::UserId) -> ServiceResult<String> {
        (**self).generate(user_id).await
    }

    async fn verify(&self, token: &str) -> ServiceResult<domain::UserId> {
        (**self).verify(token).await
    }
}
