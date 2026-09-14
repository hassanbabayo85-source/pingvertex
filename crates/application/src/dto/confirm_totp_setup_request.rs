use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ConfirmTotpSetupRequest {
    pub totp_code: String,
}

/// Raw (un-hashed) backup codes, shown to the user exactly once. The
/// server never has the raw values again after this response — only
/// their `PasswordHasher` hashes persist in `backup_codes.code_hash`.
#[derive(Clone, Serialize, Deserialize)]
pub struct ConfirmTotpSetupResponse {
    pub backup_codes: Vec<String>,
}

/// PART 7 — manual Debug that redacts the TOTP code.
impl std::fmt::Debug for ConfirmTotpSetupRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfirmTotpSetupRequest")
            .field("totp_code", &"[REDACTED]")
            .finish()
    }
}

/// PART 7 — manual Debug that redacts the raw backup codes. They are
/// shown to the user exactly once and are the account-recovery
/// credentials; nothing should ever be able to log them.
impl std::fmt::Debug for ConfirmTotpSetupResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfirmTotpSetupResponse")
            .field("backup_codes", &"[REDACTED]")
            .field("backup_codes_count", &self.backup_codes.len())
            .finish()
    }
}
