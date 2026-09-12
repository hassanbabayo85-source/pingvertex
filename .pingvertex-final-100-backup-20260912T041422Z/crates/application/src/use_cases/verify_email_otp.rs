use std::error::Error;

use chrono::Utc;

use crate::ports::{EmailVerificationTokenRepository, PasswordHasher, UserRepository};
use domain::Email;

/// Verifies the short-lived 6-digit email OTP.
///
/// The lookup is by normalized email, never by the OTP itself. The OTP is
/// Argon2-verified against the newest unused token and then atomically consumed.
/// This keeps the OTP out of logs, URLs, database lookup keys, and analytics.
pub struct VerifyEmailOtpUseCase<R, U, H> {
    token_repository: R,
    user_repository: U,
    password_hasher: H,
}

#[derive(Debug)]
pub enum VerifyEmailOtpError {
    InvalidCode,
    Internal(Box<dyn Error + Send + Sync>),
}

impl std::fmt::Display for VerifyEmailOtpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCode => write!(f, "invalid or expired verification code"),
            Self::Internal(_) => write!(f, "failed to verify email"),
        }
    }
}

impl Error for VerifyEmailOtpError {}

impl<R, U, H> VerifyEmailOtpUseCase<R, U, H>
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

    pub async fn execute(
        &self,
        email_raw: String,
        code: String,
    ) -> Result<(), VerifyEmailOtpError> {
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            return Err(VerifyEmailOtpError::InvalidCode);
        }

        let email = Email::new(email_raw).map_err(|_| VerifyEmailOtpError::InvalidCode)?;
        let user = self
            .user_repository
            .find_by_email(&email)
            .await
            .map_err(VerifyEmailOtpError::Internal)?
            .ok_or(VerifyEmailOtpError::InvalidCode)?;

        let token = self
            .token_repository
            .find_latest_unused_by_user(user.id())
            .await
            .map_err(VerifyEmailOtpError::Internal)?
            .ok_or(VerifyEmailOtpError::InvalidCode)?;

        if !token.is_valid(Utc::now()) {
            return Err(VerifyEmailOtpError::InvalidCode);
        }

        let valid = self
            .password_hasher
            .verify(&code, &token.token_hash)
            .await
            .map_err(VerifyEmailOtpError::Internal)?;

        if !valid {
            return Err(VerifyEmailOtpError::InvalidCode);
        }

        let now = Utc::now();
        let claimed = self
            .token_repository
            .try_consume(token.id, now)
            .await
            .map_err(VerifyEmailOtpError::Internal)?;

        if !claimed {
            return Err(VerifyEmailOtpError::InvalidCode);
        }

        let mut user = user;
        user.mark_email_verified(now);
        self.user_repository
            .update(&user)
            .await
            .map_err(VerifyEmailOtpError::Internal)?;

        Ok(())
    }
}
