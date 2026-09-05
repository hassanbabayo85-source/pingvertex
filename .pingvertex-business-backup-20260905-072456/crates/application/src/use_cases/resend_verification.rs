use std::error::Error;

use chrono::{Duration, Utc};

use crate::{
    dto::ResendVerificationRequest,
    ports::{
        EmailVerificationMailer, EmailVerificationTokenRepository, IdGenerator, PasswordHasher,
        UserRepository,
    },
    use_cases::email_verification_support::{
        EMAIL_VERIFICATION_TOKEN_TTL_MINUTES, email_verification_token_prefix,
    },
};

use domain::{Email, EmailVerificationToken, EmailVerificationTokenId};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Resends a verification email for an unverified account.
///
/// Enumeration protection: this use case returns `Ok(())` for invalid,
/// nonexistent, or already-verified email addresses — no token is
/// generated and no error is surfaced. Internal repository/mailer errors
/// are returned to the caller, but the HTTP handler ignores them and
/// returns the same generic response either way.
pub struct ResendVerificationUseCase<U, R, G, H, M> {
    user_repository: U,
    token_repository: R,
    id_generator: G,
    password_hasher: H,
    mailer: M,
}

impl<U, R, G, H, M> ResendVerificationUseCase<U, R, G, H, M>
where
    U: UserRepository,
    R: EmailVerificationTokenRepository,
    G: IdGenerator,
    H: PasswordHasher,
    M: EmailVerificationMailer,
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

    pub async fn execute(&self, request: ResendVerificationRequest) -> UseCaseResult<()> {
        let email = match Email::new(request.email) {
            Ok(email) => email,
            Err(_) => return Ok(()),
        };

        let user = match self.user_repository.find_by_email(&email).await {
            Ok(Some(user)) => user,
            _ => return Ok(()),
        };

        if user.is_email_verified() {
            return Ok(());
        }

        let user_id = user.id();

        let opaque_token = self.id_generator.next_email_verification_token().await;
        let token_prefix = email_verification_token_prefix(&opaque_token)?;
        let otp = self.id_generator.next_email_verification_otp().await;
        let token_hash = self.password_hasher.hash(&otp).await?;

        let now = Utc::now();
        let expires_at = now + Duration::minutes(EMAIL_VERIFICATION_TOKEN_TTL_MINUTES);

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
            .send_verification_email(&email, &otp)
            .await?;

        Ok(())
    }
}
