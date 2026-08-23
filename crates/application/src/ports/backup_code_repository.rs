use std::error::Error;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{BackupCode, BackupCodeId, UserId};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait BackupCodeRepository: Send + Sync {
    /// Persists a freshly generated batch of backup codes (always 10, see
    /// `ConfirmTotpSetupUseCase`) in one call.
    async fn create_many(&self, codes: &[BackupCode]) -> RepoResult<()>;

    /// Returns every not-yet-used backup code for `user_id`. Used by
    /// `VerifyTotpAndLoginUseCase` to check a submitted backup code
    /// against each stored hash via `PasswordHasher::verify`.
    async fn find_unused_by_user(&self, user_id: UserId) -> RepoResult<Vec<BackupCode>>;

    /// Marks a single backup code as redeemed. Must be called
    /// immediately after a successful `PasswordHasher::verify` match so
    /// the same code cannot be used twice.
    async fn mark_used(&self, id: BackupCodeId, used_at: DateTime<Utc>) -> RepoResult<()>;

    /// Deletes every backup code (used and unused) belonging to
    /// `user_id`. Called by `DisableTotpUseCase` and re-called by
    /// `ConfirmTotpSetupUseCase` before inserting a fresh batch, so
    /// re-enrolling never leaves stale codes from a previous enrollment
    /// redeemable.
    async fn delete_all_for_user(&self, user_id: UserId) -> RepoResult<()>;
}

use std::sync::Arc;

#[async_trait]
impl<T> BackupCodeRepository for Arc<T>
where
    T: BackupCodeRepository + ?Sized,
{
    async fn create_many(&self, codes: &[BackupCode]) -> RepoResult<()> {
        (**self).create_many(codes).await
    }

    async fn find_unused_by_user(&self, user_id: UserId) -> RepoResult<Vec<BackupCode>> {
        (**self).find_unused_by_user(user_id).await
    }

    async fn mark_used(&self, id: BackupCodeId, used_at: DateTime<Utc>) -> RepoResult<()> {
        (**self).mark_used(id, used_at).await
    }

    async fn delete_all_for_user(&self, user_id: UserId) -> RepoResult<()> {
        (**self).delete_all_for_user(user_id).await
    }
}
