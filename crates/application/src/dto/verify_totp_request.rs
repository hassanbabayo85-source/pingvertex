use serde::{Deserialize, Serialize};

/// `code` may be EITHER a 6-digit TOTP code OR one of the user's unused
/// backup codes — `VerifyTotpAndLoginUseCase` tries TOTP first (cheap,
/// no DB round-trip) and falls back to the backup-code table only when
/// TOTP verification fails.
#[derive(Clone, Serialize, Deserialize)]
pub struct VerifyTotpRequest {
    pub pending_token: String,
    pub code: String,
}

/// Manual Debug that redacts the pending token and TOTP code.
impl std::fmt::Debug for VerifyTotpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifyTotpRequest")
            .field("pending_token", &"[REDACTED]")
            .field("code", &"[REDACTED]")
            .finish()
    }
}
