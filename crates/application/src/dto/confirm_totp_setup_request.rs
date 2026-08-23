use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmTotpSetupRequest {
    pub totp_code: String,
}

/// Raw (un-hashed) backup codes, shown to the user exactly once. The
/// server never has the raw values again after this response — only
/// their `PasswordHasher` hashes persist in `backup_codes.code_hash`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmTotpSetupResponse {
    pub backup_codes: Vec<String>,
}
