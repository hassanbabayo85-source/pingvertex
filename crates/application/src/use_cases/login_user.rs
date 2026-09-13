use std::error::Error;

use crate::{
    dto::{AuthResponse, LoginOutcome, LoginRequest, PendingTwoFactorResponse},
    ports::{IdGenerator, PasswordHasher, PendingAuthCache, TokenService, UserRepository},
    use_cases::totp_support::{PENDING_LOGIN_TTL_SECONDS, pending_login_cache_key},
};

use domain::{Email, UserStatus};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const MAX_PASSWORD_LENGTH: usize = 128;
const MAX_FAILED_LOGIN_ATTEMPTS: u32 = 5;
const LOGIN_LOCKOUT_TTL_SECONDS: u64 = 900;

const LOGIN_LOCKOUT_MESSAGE: &str =
    "account temporarily locked due to too many failed login attempts; please try again later";

/// THE one and only client-facing authentication-failure message used
/// by this use case. Every rejection — non-existent email, wrong
/// password, pending verification, suspended, deleted, or an internal
/// cache error — returns EXACTLY this string, so no caller can
/// distinguish account state by inspecting the error.
const GENERIC_AUTH_FAILURE: &str = "invalid email or password";

const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$wYo4u5354twxR/K/HWCW7g$3Zmj6/MSGlMa/2QpgFadVgV73QxWzpuOBzsGK9Fpyug";

fn login_attempts_cache_key(email: &Email) -> String {
    format!("login_attempts:{}", email.as_str())
}

pub struct LoginUserUseCase<R, H, T, G, C> {
    repository: R,
    password_hasher: H,
    token_service: T,
    id_generator: G,
    pending_auth_cache: C,
}

impl<R, H, T, G, C> LoginUserUseCase<R, H, T, G, C>
where
    R: UserRepository,
    H: PasswordHasher,
    T: TokenService,
    G: IdGenerator,
    C: PendingAuthCache,
{
    pub fn new(
        repository: R,
        password_hasher: H,
        token_service: T,
        id_generator: G,
        pending_auth_cache: C,
    ) -> Self {
        Self {
            repository,
            password_hasher,
            token_service,
            id_generator,
            pending_auth_cache,
        }
    }

    pub async fn execute(&self, request: LoginRequest) -> UseCaseResult<LoginOutcome> {
        let email = Email::new(request.email)?;

        let attempts_key = login_attempts_cache_key(&email);
        let attempts_before_this_request: u32 = self
            .pending_auth_cache
            .get(&attempts_key)
            .await
            .ok()
            .flatten()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);

        if attempts_before_this_request >= MAX_FAILED_LOGIN_ATTEMPTS {
            return Err(LOGIN_LOCKOUT_MESSAGE.into());
        }

        if request.password.chars().count() > MAX_PASSWORD_LENGTH {
            self.record_failed_login_attempt(&attempts_key, attempts_before_this_request)
                .await;
            return Err(GENERIC_AUTH_FAILURE.into());
        }

        let user = self.repository.find_by_email(&email).await?;

        let password_hash = user
            .as_ref()
            .map(|u| u.password_hash().as_str().to_string())
            .unwrap_or_else(|| DUMMY_PASSWORD_HASH.to_string());

        let valid = self
            .password_hasher
            .verify(&request.password, &password_hash)
            .await
            .unwrap_or(false);

        if !valid || user.is_none() {
            self.record_failed_login_attempt(&attempts_key, attempts_before_this_request)
                .await;
            return Err(GENERIC_AUTH_FAILURE.into());
        }

        let _ = self.pending_auth_cache.delete(&attempts_key).await;

        let user = user.unwrap();

        // SEC: every non-Active status — including PendingVerification —
        // must return the SAME generic authentication-failure string as
        // a wrong password. A distinct "email verification required"
        // response would turn login into an account-status enumeration
        // oracle.
        match user.status() {
            UserStatus::Active => {}
            UserStatus::PendingVerification => {
                self.record_failed_login_attempt(&attempts_key, attempts_before_this_request)
                    .await;
                return Err(GENERIC_AUTH_FAILURE.into());
            }
            UserStatus::Suspended => {
                return Err(GENERIC_AUTH_FAILURE.into());
            }
            UserStatus::Deleted => {
                return Err(GENERIC_AUTH_FAILURE.into());
            }
        }

        if user.totp_enabled() {
            let pending_token = self.id_generator.next_pending_two_factor_token().await;

            self.pending_auth_cache
                .set(
                    &pending_login_cache_key(&pending_token),
                    &user.id().to_string(),
                    PENDING_LOGIN_TTL_SECONDS,
                )
                .await
                .map_err(|_| GENERIC_AUTH_FAILURE)?;

            return Ok(LoginOutcome::PendingTwoFactor(PendingTwoFactorResponse {
                pending_token,
            }));
        }

        let token = self.token_service.generate(user.id()).await?;

        Ok(LoginOutcome::Authenticated(AuthResponse {
            access_token: token,
        }))
    }

    async fn record_failed_login_attempt(&self, key: &str, attempts_before_this_request: u32) {
        let next = attempts_before_this_request + 1;
        let _ = self
            .pending_auth_cache
            .set(key, &next.to_string(), LOGIN_LOCKOUT_TTL_SECONDS)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_attempts_cache_key_is_namespaced_and_uses_normalized_email() {
        let a = Email::new("Foo@Bar.com").unwrap();
        let b = Email::new("  foo@bar.com  ").unwrap();

        let key_a = login_attempts_cache_key(&a);
        let key_b = login_attempts_cache_key(&b);

        assert_eq!(
            key_a, key_b,
            "case/whitespace variations must share one counter"
        );
        assert!(key_a.starts_with("login_attempts:"));
        assert!(key_a.contains("foo@bar.com"));
    }
}
