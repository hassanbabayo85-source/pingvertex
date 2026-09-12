use std::error::Error;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{ResetPasswordToken, ResetPasswordTokenId, UserId};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for `ResetPasswordToken`.
///
/// Mirrors `EmailVerificationTokenRepository`'s "hash at rest" contract:
/// the raw token is never persisted — only `ResetPasswordToken::token_hash`
/// (Argon2id) and the plaintext lookup prefix are stored.
#[async_trait]
pub trait ResetPasswordTokenRepository: Send + Sync {
    async fn create(&self, token: &ResetPasswordToken) -> RepoResult<()>;

    /// Lookup by plaintext prefix. The prefix is NOT unique, so callers
    /// must re-verify the full raw token against `token_hash` before
    /// accepting the result.
    async fn find_by_prefix(&self, token_prefix: &str) -> RepoResult<Option<ResetPasswordToken>>;

    /// Marks a token as redeemed. Must only be called after a successful
    /// full-token verification so replay is impossible.
    async fn mark_used(&self, id: ResetPasswordTokenId, used_at: DateTime<Utc>) -> RepoResult<()>;

    /// SECURITY: atomic single-use claim. Marks the token used ONLY if it
    /// is still unused (`used_at IS NULL`), in one atomic statement, and
    /// returns whether THIS call was the one that claimed it.
    ///
    /// Must be called (and its `true` result checked) BEFORE the password
    /// is actually changed. Without this, two concurrent requests can
    /// both pass the earlier `find_by_prefix` + hash-verify checks before
    /// either calls `mark_used`, letting the same token be redeemed twice
    /// in a race. Doing the claim first and bailing out on `false` closes
    /// that window: only one concurrent caller can ever proceed past this
    /// point for a given token.
    async fn try_consume(
        &self,
        id: ResetPasswordTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<bool>;

    /// Deletes all unused tokens for a user. Called both when issuing a
    /// fresh token (so only the latest link remains redeemable) and
    /// after a successful reset (so an older, still-unexpired link
    /// can't also be redeemed afterward).
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

    async fn delete_unused_for_user(&self, user_id: UserId) -> RepoResult<()> {
        (**self).delete_unused_for_user(user_id).await
    }
}
