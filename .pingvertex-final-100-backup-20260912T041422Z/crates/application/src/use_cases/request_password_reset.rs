use std::error::Error;

use chrono::{Duration, Utc};

use crate::{
    dto::ForgotPasswordRequest,
    ports::{
        IdGenerator, PasswordHasher, PasswordResetMailer, ResetPasswordTokenRepository,
        UserRepository,
    },
    use_cases::reset_password_support::{
        RESET_PASSWORD_TOKEN_TTL_MINUTES, reset_password_token_prefix,
    },
};

use domain::{Email, ResetPasswordToken, ResetPasswordTokenId};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Initiates a password reset for an account identified by email
/// ("forgot password").
///
/// Enumeration protection: returns `Ok(())` uniformly whether the
/// supplied email is malformed, doesn't correspond to any account, or
/// corresponds to a perfectly ordinary one — a reset token is only ever
/// generated in the last case. The HTTP handler
/// (`crates/api/src/handlers/forgot_password.rs`) returns the exact same
/// response body regardless, and swallows any `Err` this method does
/// return (which only ever originates from a genuine repository/mailer
/// failure, never from "no such account") — mirrors
/// `ResendVerificationUseCase`'s identical precedent exactly.
pub struct RequestPasswordResetUseCase<U, R, G, H, M> {
    user_repository: U,
    token_repository: R,
    id_generator: G,
    password_hasher: H,
    mailer: M,
}

impl<U, R, G, H, M> RequestPasswordResetUseCase<U, R, G, H, M>
where
    U: UserRepository,
    R: ResetPasswordTokenRepository,
    G: IdGenerator,
    H: PasswordHasher,
    M: PasswordResetMailer,
{
    pub fn new(
        user_repository: U,
        token_repository: R,
        id_generator: G,
        password_hasher: H,
        mailer: M,
    ) -> Self {
        Self {
            user_repository,
            token_repository,
            id_generator,
            password_hasher,
            mailer,
        }
    }

    pub async fn execute(&self, request: ForgotPasswordRequest) -> UseCaseResult<()> {
        let email = match Email::new(request.email) {
            Ok(email) => email,
            Err(_) => return Ok(()),
        };

        let user = match self.user_repository.find_by_email(&email).await {
            Ok(Some(user)) => user,
            _ => return Ok(()),
        };

        let user_id = user.id();

        let raw_token = self.id_generator.next_reset_password_token().await;
        let token_prefix = reset_password_token_prefix(&raw_token)?;
        let token_hash = self.password_hasher.hash(&raw_token).await?;

        let now = Utc::now();
        let expires_at = now + Duration::minutes(RESET_PASSWORD_TOKEN_TTL_MINUTES);

        let token = ResetPasswordToken::new(
            ResetPasswordTokenId::new(),
            user_id,
            token_prefix,
            token_hash,
            expires_at,
            now,
        );

        // Invalidate any previously issued, still-unused reset tokens for
        // this user first, so only the newest one is ever redeemable —
        // mirrors CreateEmailVerificationTokenUseCase.
        self.token_repository
            .delete_unused_for_user(user_id)
            .await?;
        self.token_repository.create(&token).await?;
        self.mailer
            .send_password_reset_email(&email, &raw_token)
            .await?;

        Ok(())
    }
}
