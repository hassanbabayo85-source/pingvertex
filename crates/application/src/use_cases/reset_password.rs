use std::error::Error;

use chrono::Utc;

use crate::{
    ports::{PasswordHasher, PendingAuthCache, ResetPasswordTokenRepository, UserRepository},
    use_cases::{
        reset_password_support::reset_password_token_prefix,
        session_invalidation::{
            SESSION_INVALIDATION_TTL_SECONDS, session_invalidated_at_cache_key,
            session_revocation_timestamp,
        },
    },
};

use domain::PasswordHash;

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

        // STEP 1 — session invalidation FIRST (fail-closed).
        // Uses session_revocation_timestamp(now) = now + 1 so a JWT
        // minted in the same Unix second as this reset (iat == now)
        // is still rejected by jwt_auth's `iat < invalidated_at` check.
        // If Redis is down, we abort BEFORE any state change.
        self.pending_auth_cache
            .set(
                &session_invalidated_at_cache_key(token.user_id),
                &session_revocation_timestamp(now.timestamp()).to_string(),
                SESSION_INVALIDATION_TTL_SECONDS,
            )
            .await
            .map_err(ResetPasswordError::Internal)?;

        // STEP 2 — atomic single-use claim of the token.
        let claimed = self
            .token_repository
            .try_consume(token.id, now)
            .await
            .map_err(ResetPasswordError::Internal)?;

        if !claimed {
            return Err(ResetPasswordError::InvalidOrExpiredToken);
        }

        let hashed = self
            .password_hasher
            .hash(&new_password)
            .await
            .map_err(ResetPasswordError::Internal)?;

        let password_hash =
            PasswordHash::new(hashed).map_err(|e| ResetPasswordError::Internal(Box::new(e)))?;

        user.set_password_hash(password_hash, now);

        // STEP 3 — persist the password change. If the durable write
        // fails, roll the token claim back via release_consumed so the
        // user is NOT stranded with a consumed-but-unredeemed token.
        if let Err(err) = self.user_repository.update(&user).await {
            let _ = self.token_repository.release_consumed(token.id).await;
            return Err(ResetPasswordError::Internal(err));
        }

        // STEP 4 — drop every other unused reset token for the account.
        self.token_repository
            .delete_unused_for_user(token.user_id)
            .await
            .map_err(ResetPasswordError::Internal)?;

        Ok(())
    }
}
