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

    /// Returns every backup code for `user_id`, both unused and already
    /// redeemed. `VerifyTotpAndLoginUseCase` needs the full set so it can
    /// distinguish "no such code / hash mismatch" from "the code exists
    /// but was already used by a concurrent request" — the latter must
    /// produce the same generic `InvalidBackupCode` error, not a TOTP error.
    async fn find_all_by_user(&self, user_id: UserId) -> RepoResult<Vec<BackupCode>>;

    /// Marks a single backup code as redeemed **only if it is still
    /// unused**, atomically. The conditional `used_at IS NULL` guard and
    /// the returned boolean make this safe against concurrent redemption
    /// races: exactly one caller receives `true` for a given code id.
    /// Implementations must return `false` (not an error) if the code was
    /// already redeemed by another request.
    async fn mark_used(&self, id: BackupCodeId, used_at: DateTime<Utc>) -> RepoResult<bool>;

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

    async fn find_all_by_user(&self, user_id: UserId) -> RepoResult<Vec<BackupCode>> {
        (**self).find_all_by_user(user_id).await
    }

    async fn mark_used(&self, id: BackupCodeId, used_at: DateTime<Utc>) -> RepoResult<bool> {
        (**self).mark_used(id, used_at).await
    }

    async fn delete_all_for_user(&self, user_id: UserId) -> RepoResult<()> {
        (**self).delete_all_for_user(user_id).await
    }
}
