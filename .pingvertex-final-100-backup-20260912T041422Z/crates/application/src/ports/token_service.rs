use std::error::Error;

use async_trait::async_trait;
use domain::UserId;

type ServiceResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Result of successfully verifying a session JWT: the user it was
/// issued for, plus WHEN it was issued (unix seconds, from the token's
/// `iat` claim).
///
/// `issued_at` was added for B6 (password-reset session invalidation):
/// `jwt_auth` compares it against a per-user "invalidated_before" marker
/// (`application::session_invalidated_at_cache_key`) written to Redis by
/// `ResetPasswordUseCase`, so a token signed under an old password stops
/// authenticating even though its signature and `exp` are still valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedToken {
    pub user_id: UserId,
    pub issued_at: i64,
}

#[async_trait]
pub trait TokenService: Send + Sync {
    async fn generate(&self, user_id: UserId) -> ServiceResult<String>;

    async fn verify(&self, token: &str) -> ServiceResult<VerifiedToken>;
}

use std::sync::Arc;

#[async_trait]
impl<T> TokenService for Arc<T>
where
    T: TokenService + ?Sized,
{
    async fn generate(&self, user_id: domain::UserId) -> ServiceResult<String> {
        (**self).generate(user_id).await
    }

    async fn verify(&self, token: &str) -> ServiceResult<VerifiedToken> {
        (**self).verify(token).await
    }
}
