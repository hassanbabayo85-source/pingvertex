use serde::{Deserialize, Serialize};

/// Requires the account's current password as a step-up re-authentication
/// check (see `DisableTotpUseCase`) — turning off 2FA is exactly as
/// sensitive an action as changing the password itself, so a valid
/// session JWT alone (`jwt_auth`) is not treated as sufficient proof of
/// intent.
#[derive(Clone, Serialize, Deserialize)]
pub struct DisableTotpRequest {
    pub password: String,
}

/// PART 7 — manual Debug that redacts the step-up password.
impl std::fmt::Debug for DisableTotpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DisableTotpRequest")
            .field("password", &"[REDACTED]")
            .finish()
    }
}
