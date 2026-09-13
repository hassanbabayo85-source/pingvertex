use std::error::Error;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{ResetPasswordToken, ResetPasswordTokenId, UserId};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait ResetPasswordTokenRepository: Send + Sync {
    async fn create(&self, token: &ResetPasswordToken) -> RepoResult<()>;

    async fn find_by_prefix(&self, token_prefix: &str) -> RepoResult<Option<ResetPasswordToken>>;

    async fn mark_used(&self, id: ResetPasswordTokenId, used_at: DateTime<Utc>) -> RepoResult<()>;

    /// Atomic single-use claim. Marks the token used ONLY if it is still
    /// unused, in one atomic statement, and returns whether THIS call
    /// was the one that claimed it.
    async fn try_consume(
        &self,
        id: ResetPasswordTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<bool>;

    /// Compensating action for a `try_consume` whose downstream password
    /// write failed. Marks the token as unused again so the caller can
    /// retry with the SAME link instead of being stranded with a
    /// consumed token and an unchanged password.
    ///
    /// MUST NEVER be called after a successful password change.
    async fn release_consumed(&self, id: ResetPasswordTokenId) -> RepoResult<()>;

    async fn delete_unused_for_user(&self, user_id: UserId) -> RepoResult<()>;
}

use std::sync::Arc;

#[async_trait]
impl<T> ResetPasswordTokenRepository for Arc<T>
where
    T: ResetPasswordTokenRepository + ?Sized,
{
    async fn create(&self, token: &ResetPasswordToken) -> RepoResult<()> {
        (**self).create(token).await
    }

    async fn find_by_prefix(&self, token_prefix: &str) -> RepoResult<Option<ResetPasswordToken>> {
        (**self).find_by_prefix(token_prefix).await
    }

    async fn mark_used(&self, id: ResetPasswordTokenId, used_at: DateTime<Utc>) -> RepoResult<()> {
        (**self).mark_used(id, used_at).await
    }

    async fn try_consume(
        &self,
        id: ResetPasswordTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<bool> {
        (**self).try_consume(id, used_at).await
    }

    async fn release_consumed(&self, id: ResetPasswordTokenId) -> RepoResult<()> {
        (**self).release_consumed(id).await
    }

    async fn delete_unused_for_user(&self, user_id: UserId) -> RepoResult<()> {
        (**self).delete_unused_for_user(user_id).await
    }
}
