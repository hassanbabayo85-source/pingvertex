use std::error::Error;

use chrono::Utc;

use crate::{
    ports::{PasswordHasher, PendingAuthCache, UserRepository},
    use_cases::session_invalidation::{
        SESSION_INVALIDATION_TTL_SECONDS, session_invalidated_at_cache_key,
        session_revocation_timestamp,
    },
};

use domain::{PasswordHash, UserId};

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
pub enum ChangePasswordError {
    InvalidCurrentPassword,
    WeakPassword(&'static str),
    Internal(Box<dyn Error + Send + Sync>),
}

impl std::fmt::Display for ChangePasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCurrentPassword => write!(f, "invalid current password"),
            Self::WeakPassword(msg) => write!(f, "{msg}"),
            Self::Internal(_) => write!(f, "failed to change password"),
        }
    }
}

impl Error for ChangePasswordError {}

pub struct ChangePasswordUseCase<R, H, C> {
    user_repository: R,
    password_hasher: H,
    pending_auth_cache: C,
}

impl<R, H, C> ChangePasswordUseCase<R, H, C>
where
    R: UserRepository,
    H: PasswordHasher,
    C: PendingAuthCache,
{
    pub fn new(user_repository: R, password_hasher: H, pending_auth_cache: C) -> Self {
        Self {
            user_repository,
            password_hasher,
            pending_auth_cache,
        }
    }

    pub async fn execute(
        &self,
        user_id: UserId,
        current_password: String,
        new_password: String,
    ) -> Result<(), ChangePasswordError> {
        validate_password(&new_password).map_err(ChangePasswordError::WeakPassword)?;

        let mut user = self
            .user_repository
            .find_by_id(&user_id)
            .await
            .map_err(ChangePasswordError::Internal)?
            .ok_or_else(|| {
                ChangePasswordError::Internal(Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "user not found",
                )))
            })?;

        let valid = self
            .password_hasher
            .verify(&current_password, user.password_hash().as_str())
            .await
            .unwrap_or(false);

        if !valid {
            return Err(ChangePasswordError::InvalidCurrentPassword);
        }

        let now = Utc::now();

        let hashed = self
            .password_hasher
            .hash(&new_password)
            .await
            .map_err(ChangePasswordError::Internal)?;

        let password_hash =
            PasswordHash::new(hashed).map_err(|e| ChangePasswordError::Internal(Box::new(e)))?;

        self.pending_auth_cache
            .set(
                &session_invalidated_at_cache_key(user_id),
                &session_revocation_timestamp(now.timestamp()).to_string(),
                SESSION_INVALIDATION_TTL_SECONDS,
            )
            .await
            .map_err(ChangePasswordError::Internal)?;

        user.set_password_hash(password_hash, now);

        self.user_repository
            .update(&user)
            .await
            .map_err(ChangePasswordError::Internal)?;

        Ok(())
    }
}
