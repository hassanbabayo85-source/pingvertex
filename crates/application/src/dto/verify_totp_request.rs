use serde::{Deserialize, Serialize};

/// `code` may be EITHER a 6-digit TOTP code OR one of the user's unused
/// backup codes — `VerifyTotpAndLoginUseCase` tries TOTP first (cheap,
/// no DB round-trip) and falls back to the backup-code table only when
/// TOTP verification fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyTotpRequest {
    pub pending_token: String,
    pub code: String,
}
