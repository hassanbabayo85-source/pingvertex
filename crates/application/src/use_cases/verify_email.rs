use std::error::Error;

use chrono::Utc;

use crate::{
    ports::{EmailVerificationTokenRepository, PasswordHasher, UserRepository},
    use_cases::email_verification_support::email_verification_token_prefix,
};

/// Error returned by `VerifyEmailUseCase`.
#[derive(Debug)]
pub enum VerifyEmailError {
    InvalidOrExpiredToken,
    Internal(Box<dyn Error + Send + Sync>),
}

impl std::fmt::Display for VerifyEmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOrExpiredToken => write!(f, "invalid or expired verification token"),
            Self::Internal(_) => write!(f, "failed to verify email"),
        }
    }
}

impl Error for VerifyEmailError {}

/// Redeems a raw email-verification token.
///
/// The raw token is looked up by its prefix, then the full token is
/// re-verified against the stored Argon2id hash before being accepted.
/// On success, the token is marked used and the user is marked
/// `email_verified_at = now` and `status = Active`.
pub struct VerifyEmailUseCase<R, U, H> {
    token_repository: R,
    user_repository: U,
    password_hasher: H,
}

impl<R, U, H> VerifyEmailUseCase<R, U, H>
where
    R: EmailVerificationTokenRepository,
    U: UserRepository,
    H: PasswordHasher,
{
    pub fn new(token_repository: R, user_repository: U, password_hasher: H) -> Self {
        Self {
            token_repository,
            user_repository,
            password_hasher,
        }
    }

    pub async fn execute(&self, raw_token: String) -> Result<(), VerifyEmailError> {
        let prefix = email_verification_token_prefix(&raw_token)
            .map_err(|_| VerifyEmailError::InvalidOrExpiredToken)?;

        let token = self
            .token_repository
            .find_by_prefix(&prefix)
            .await
            .map_err(VerifyEmailError::Internal)?
            .ok_or(VerifyEmailError::InvalidOrExpiredToken)?;

        if !token.is_valid(Utc::now()) {
            return Err(VerifyEmailError::InvalidOrExpiredToken);
        }

        let token_user_id = token.user_id;
        let token_id = token.id;
        let token_hash = token.token_hash.clone();

        let valid = self
            .password_hasher
            .verify(&raw_token, &token_hash)
            .await
            .map_err(VerifyEmailError::Internal)?;

        if !valid {
            return Err(VerifyEmailError::InvalidOrExpiredToken);
        }

        let mut user = self
            .user_repository
            .find_by_id(&token_user_id)
            .await
            .map_err(VerifyEmailError::Internal)?
            .ok_or(VerifyEmailError::InvalidOrExpiredToken)?;

        let mut token = token;
        token.mark_used(Utc::now());
        self.token_repository
            .mark_used(token_id, Utc::now())
            .await
            .map_err(VerifyEmailError::Internal)?;

        user.mark_email_verified(Utc::now());
        self.user_repository
            .update(&user)
            .await
            .map_err(VerifyEmailError::Internal)?;

        Ok(())
    }
}
