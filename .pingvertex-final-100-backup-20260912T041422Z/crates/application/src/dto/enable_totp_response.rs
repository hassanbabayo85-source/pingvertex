use serde::{Deserialize, Serialize};

/// Returned by `EnableTotpUseCase`. `otpauth_uri` is an `otpauth://totp/...`
/// URI (RFC 6238 / Key URI Format) the client renders as a QR code for the
/// user to scan into their authenticator app. `totp_enabled` is NOT yet
/// `true` on the account at this point — it only flips on once
/// `ConfirmTotpSetupUseCase` verifies the user's first code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnableTotpResponse {
    pub otpauth_uri: String,
}
