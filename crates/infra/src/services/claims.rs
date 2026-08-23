use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    /// Unix timestamp (seconds) this token was issued at.
    ///
    /// Added for B6 (password-reset session invalidation): `jwt_auth`
    /// compares this against a per-user "invalidated_before" marker
    /// written to Redis by `ResetPasswordUseCase`, so a JWT signed under
    /// an old password stops authenticating even though its signature
    /// and `exp` are still technically valid. See
    /// `application::session_invalidated_at_cache_key`.
    pub iat: usize,
    pub exp: usize,
}
