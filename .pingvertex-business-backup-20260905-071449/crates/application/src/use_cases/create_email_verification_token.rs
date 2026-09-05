use std::error::Error;

use chrono::{Duration, Utc};

use crate::{
    ports::{
        EmailVerificationMailer, EmailVerificationTokenRepository, IdGenerator, PasswordHasher,
    },
    use_cases::email_verification_support::{
        EMAIL_VERIFICATION_TOKEN_TTL_HOURS, email_verification_token_prefix,
    },
};

use domain::{Email, EmailVerificationToken, EmailVerificationTokenId, UserId};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Creates a fresh email-verification token for `user_id`, invalidating
/// any previous unused tokens, and emails the raw token to `email`.
///
/// SECURITY:
/// - The raw token is never logged.
/// - Only the Argon2id hash is persisted (`EmailVerificationToken.token_hash`).
/// - The plaintext prefix stored is only a lookup aid, never sufficient
///   for verification by itself.
pub struct CreateEmailVerificationTokenUseCase<R, G, H, M> {
    token_repository: R,
    id_generator: G,
    password_hasher: H,
    mailer: M,
}

impl<R, G, H, M> CreateEmailVerificationTokenUseCase<R, G, H, M>
where
    R: EmailVerificationTokenRepository,
    G: IdGenerator,
    H: PasswordHasher,
    M: EmailVerificationMailer,
{
    pub fn new(token_repository: R, id_generator: G, password_hasher: H, mailer: M) -> Self {
        Self {
            token_repository,
            id_generator,
            password_hasher,
            mailer,
        }
    }

    pub async fn execute(&self, user_id: UserId, email: &Email) -> UseCaseResult<()> {
        let raw_token = self.id_generator.next_email_verification_token().await;
        let token_prefix = email_verification_token_prefix(&raw_token)?;
        let token_hash = self.password_hasher.hash(&raw_token).await?;

        let now = Utc::now();
        let expires_at = now + Duration::hours(EMAIL_VERIFICATION_TOKEN_TTL_HOURS);

        let token = EmailVerificationToken::new(
            EmailVerificationTokenId::new(),
            user_id,
            token_prefix,
            token_hash,
            expires_at,
            now,
        );

        self.token_repository
            .delete_unused_for_user(user_id)
            .await?;
        self.token_repository.create(&token).await?;
        self.mailer
            .send_verification_email(email, &raw_token)
            .await?;

        Ok(())
    }
}
