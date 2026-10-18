use serde::{Deserialize, Serialize};

/// Returned by `LoginUserUseCase::execute` INSTEAD of `AuthResponse` when
/// `user.totp_enabled()` is true. `pending_token` proves the caller
/// already presented a correct email/password pair, but does NOT
/// authenticate API requests by itself — it is only valid for
/// `POST /auth/totp/verify` and expires after 5 minutes (see
/// `PendingAuthCache`).
#[derive(Clone, Serialize, Deserialize)]
pub struct PendingTwoFactorResponse {
    pub pending_token: String,
}

/// The two possible successful outcomes of `LoginUserUseCase::execute`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoginOutcome {
    Authenticated(super::AuthResponse),
    PendingTwoFactor(PendingTwoFactorResponse),
}

/// Manual Debug that redacts the pending 2FA token.
impl std::fmt::Debug for PendingTwoFactorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingTwoFactorResponse")
            .field("pending_token", &"[REDACTED]")
            .finish()
    }
}
