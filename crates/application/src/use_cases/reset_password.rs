use std::error::Error;

use chrono::Utc;

use crate::{
    ports::{PasswordHasher, PendingAuthCache, ResetPasswordTokenRepository, UserRepository},
    use_cases::{
        reset_password_support::reset_password_token_prefix,
        session_invalidation::{
            SESSION_INVALIDATION_TTL_SECONDS, session_invalidated_at_cache_key,
        },
    },
};

use domain::PasswordHash;

/// Password policy, deliberately duplicated from (rather than imported
/// out of) `register_user.rs`'s identical, already-built
/// `validate_password`: that function is private to a different,
/// already-completed worker's file, and this worker's SCOPE DISCIPLINE
/// rule is to not modify another worker's already-built module just to
/// extract a one-line-saving shared helper. See HANDOVER.md "Deviations
/// from prompt". If the two ever need to diverge (e.g. a stricter reset
/// policy), that's a deliberate product decision for a future worker,
/// not an accident of code reuse.
const MIN_PASSWORD_LENGTH: usize = 12;
const MAX_PASSWORD_LENGTH: usize = 128;

fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.chars().count() > MAX_PASSWORD_LENGTH {
        return Err("password must not exceed 128 characters");
    }
    if password.trim().is_empty() {
        return Err("password must not be empty");
    }
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err("password must be at least 12 characters long");
    }
    if !password.chars().any(|c| c.is_ascii_uppercase()) {
        return Err("password must contain at least one uppercase letter");
    }
    if !password.chars().any(|c| c.is_ascii_lowercase()) {
        return Err("password must contain at least one lowercase letter");
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        return Err("password must contain at least one digit");
    }
    if !password.chars().any(|c| !c.is_ascii_alphanumeric()) {
        return Err("password must contain at least one special character");
    }
    Ok(())
}

/// Error returned by `ResetPasswordUseCase`.
#[derive(Debug)]
pub enum ResetPasswordError {
    InvalidOrExpiredToken,
    WeakPassword(&'static str),
    Internal(Box<dyn Error + Send + Sync>),
}

impl std::fmt::Display for ResetPasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOrExpiredToken => write!(f, "invalid or expired reset token"),
            Self::WeakPassword(msg) => write!(f, "{msg}"),
            Self::Internal(_) => write!(f, "failed to reset password"),
        }
    }
}

impl Error for ResetPasswordError {}

/// Redeems a raw password-reset token and sets a new password.
///
/// The raw token is looked up by its prefix, then the full token is
/// re-verified against the stored Argon2id hash before being accepted
/// (identical pattern to `VerifyEmailUseCase`). On success:
///   1. the user's password hash is replaced,
///   2. the token is marked used and every other unused reset token for
///      the account is deleted,
///   3. a "session invalidated before now" marker is written to
///      `PendingAuthCache` (Redis) so `jwt_auth` starts rejecting every
///      session JWT that was issued before this moment (see
///      `application::session_invalidated_at_cache_key`).
///
/// Step 3 is best-effort: a `PendingAuthCache` failure does not fail the
/// password reset itself (the new password is already durably saved by
/// that point) — see the inline comment at the call site for the full
/// security/availability trade-off.
pub struct ResetPasswordUseCase<R, U, H, C> {
    token_repository: R,
    user_repository: U,
    password_hasher: H,
    pending_auth_cache: C,
}

impl<R, U, H, C> ResetPasswordUseCase<R, U, H, C>
where
    R: ResetPasswordTokenRepository,
    U: UserRepository,
    H: PasswordHasher,
    C: PendingAuthCache,
{
    pub fn new(
        token_repository: R,
        user_repository: U,
        password_hasher: H,
        pending_auth_cache: C,
    ) -> Self {
        Self {
            token_repository,
            user_repository,
            password_hasher,
            pending_auth_cache,
        }
    }

    pub async fn execute(
        &self,
        raw_token: String,
        new_password: String,
    ) -> Result<(), ResetPasswordError> {
        validate_password(&new_password).map_err(ResetPasswordError::WeakPassword)?;

        let prefix = reset_password_token_prefix(&raw_token)
            .map_err(|_| ResetPasswordError::InvalidOrExpiredToken)?;

        let token = self
            .token_repository
            .find_by_prefix(&prefix)
            .await
            .map_err(ResetPasswordError::Internal)?
            .ok_or(ResetPasswordError::InvalidOrExpiredToken)?;

        if !token.is_valid(Utc::now()) {
            return Err(ResetPasswordError::InvalidOrExpiredToken);
        }

        let valid = self
            .password_hasher
            .verify(&raw_token, &token.token_hash)
            .await
            .map_err(ResetPasswordError::Internal)?;

        if !valid {
            return Err(ResetPasswordError::InvalidOrExpiredToken);
        }

        let mut user = self
            .user_repository
            .find_by_id(&token.user_id)
            .await
            .map_err(ResetPasswordError::Internal)?
            .ok_or(ResetPasswordError::InvalidOrExpiredToken)?;

        let now = Utc::now();

        let hashed = self
            .password_hasher
            .hash(&new_password)
            .await
            .map_err(ResetPasswordError::Internal)?;

        let password_hash =
            PasswordHash::new(hashed).map_err(|e| ResetPasswordError::Internal(Box::new(e)))?;

        user.set_password_hash(password_hash, now);

        self.user_repository
            .update(&user)
            .await
            .map_err(ResetPasswordError::Internal)?;

        // Single-use: mark THIS token used, and drop any other unused
        // reset tokens for the account so an older, still-unexpired link
        // can't also be redeemed afterward.
        self.token_repository
            .mark_used(token.id, now)
            .await
            .map_err(ResetPasswordError::Internal)?;
        self.token_repository
            .delete_unused_for_user(token.user_id)
            .await
            .map_err(ResetPasswordError::Internal)?;

        // SEC (B6): invalidate every session JWT issued to this user
        // BEFORE this moment. Best-effort — deliberately NOT propagated
        // as an `Err` on failure: the new password is already durably
        // saved above, so failing the whole request here would leave the
        // account in a confusing state (password changed, but the client
        // told "reset failed"). A `PendingAuthCache` outage instead just
        // means pre-existing sessions keep working until they expire
        // naturally (max 24h) rather than being cut off immediately —
        // the same fail-open posture this cache already has elsewhere
        // (e.g. TOTP pending-secret storage). See `jwt_auth.rs` for the
        // read side of this check.
        let _ = self
            .pending_auth_cache
            .set(
                &session_invalidated_at_cache_key(token.user_id),
                &now.timestamp().to_string(),
                SESSION_INVALIDATION_TTL_SECONDS,
            )
            .await;

        Ok(())
    }
}
