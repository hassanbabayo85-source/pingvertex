use async_trait::async_trait;
use domain::UserId;
use std::{error::Error, sync::Arc};

pub type OAuthIdentityRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait OAuthIdentityRepository: Send + Sync {
    async fn find_user_id(
        &self,
        provider: &str,
        subject: &str,
    ) -> OAuthIdentityRepoResult<Option<UserId>>;
    async fn link(
        &self,
        provider: &str,
        subject: &str,
        user_id: UserId,
    ) -> OAuthIdentityRepoResult<()>;
}

#[async_trait]
impl<T> OAuthIdentityRepository for Arc<T>
where
    T: OAuthIdentityRepository + Send + Sync + ?Sized,
{
    async fn find_user_id(
        &self,
        provider: &str,
        subject: &str,
    ) -> OAuthIdentityRepoResult<Option<UserId>> {
        (**self).find_user_id(provider, subject).await
    }
    async fn link(
        &self,
        provider: &str,
        subject: &str,
        user_id: UserId,
    ) -> OAuthIdentityRepoResult<()> {
        (**self).link(provider, subject, user_id).await
    }
}
