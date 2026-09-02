use std::error::Error;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{EmailVerificationToken, EmailVerificationTokenId, UserId};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for `EmailVerificationToken`.
///
/// Mirrors `BackupCodeRepository`'s "hash at rest" contract: the raw
/// token is never persisted — only `EmailVerificationToken::token_hash`
/// (Argon2id) and the plaintext lookup prefix are stored.
#[async_trait]
pub trait EmailVerificationTokenRepository: Send + Sync {
    async fn create(&self, token: &EmailVerificationToken) -> RepoResult<()>;

    /// Lookup by plaintext prefix. The prefix is NOT unique (see
    /// migration `0034`), so callers must re-verify the full raw token
    /// against `token_hash` before accepting the result.
    async fn find_by_prefix(
        &self,
        token_prefix: &str,
    ) -> RepoResult<Option<EmailVerificationToken>>;

    /// Marks a token as redeemed. Must only be called after a successful
    /// full-token verification so replay is impossible.
    async fn mark_used(
        &self,
        id: EmailVerificationTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<()>;

    /// SECURITY: atomic single-use claim, mirroring
    /// `ResetPasswordTokenRepository::try_consume`. Marks the token used
    /// ONLY if it is still unused, in one atomic statement, and returns
    /// whether THIS call was the one that claimed it. Must be called
    /// (and its `true` result checked) BEFORE `user.mark_email_verified`
    /// is applied, so two concurrent requests racing on the same token
    /// can't both pass verification before either consumes it.
    async fn try_consume(
        &self,
        id: EmailVerificationTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<bool>;

    /// Deletes all unused tokens for a user. Called when issuing a fresh
    /// token (resend or initial registration), so only the latest token
    /// remains redeemable.
    async fn delete_unused_for_user(&self, user_id: UserId) -> RepoResult<()>;
}

use std::sync::Arc;

#[async_trait]
impl<T> EmailVerificationTokenRepository for Arc<T>
where
    T: EmailVerificationTokenRepository + ?Sized,
{
    async fn create(&self, token: &EmailVerificationToken) -> RepoResult<()> {
        (**self).create(token).await
    }

    async fn find_by_prefix(
        &self,
        token_prefix: &str,
    ) -> RepoResult<Option<EmailVerificationToken>> {
        (**self).find_by_prefix(token_prefix).await
    }

    async fn mark_used(
        &self,
        id: EmailVerificationTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        (**self).mark_used(id, used_at).await
    }

    async fn try_consume(
        &self,
        id: EmailVerificationTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<bool> {
        (**self).try_consume(id, used_at).await
    }

    async fn delete_unused_for_user(&self, user_id: UserId) -> RepoResult<()> {
        (**self).delete_unused_for_user(user_id).await
    }
}
