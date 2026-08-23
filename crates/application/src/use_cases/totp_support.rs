use domain::UserId;
use totp_rs::{Algorithm, Secret, TOTP};

/// Issuer name shown inside authenticator apps (Google Authenticator,
/// Authy, 1Password, ...) next to the account name.
pub(crate) const ISSUER: &str = "PingVertex";

/// TTL for a not-yet-confirmed TOTP secret sitting in `PendingAuthCache`,
/// keyed by user id. Matches the task's "TTL min 10" (minutes)
/// requirement for pending secrets.
pub(crate) const PENDING_SECRET_TTL_SECONDS: u64 = 600;

/// TTL for the opaque pending-2FA-login token `LoginUserUseCase` hands
/// out. Matches the task's "TTL min 5" (minutes) requirement for pending
/// 2FA tokens.
pub(crate) const PENDING_LOGIN_TTL_SECONDS: u64 = 300;

/// Redis (via `PendingAuthCache`) key for a user's pending, unconfirmed
/// TOTP secret. Namespaced separately from `pending_login_cache_key`
/// below even though both share the same underlying cache/connection.
pub(crate) fn pending_secret_cache_key(user_id: UserId) -> String {
    format!("totp:pending_secret:{user_id}")
}

/// Redis (via `PendingAuthCache`) key for a pending 2FA login token,
/// mapping the random opaque token to the user id it was issued for.
pub(crate) fn pending_login_cache_key(pending_token: &str) -> String {
    format!("totp:pending_login:{pending_token}")
}

/// Builds a `totp_rs::TOTP` from a raw (already decrypted) secret, with
/// RFC 6238 defaults: SHA1, 6 digits, 30 second step, and a skew of 1
/// step in either direction (i.e. the ±1 step / ±30s clock-skew
/// tolerance required by the task's SECURITY REQUIREMENTS —
/// `check_current` below accepts the previous, current, or next step).
pub(crate) fn build_totp(secret_bytes: Vec<u8>, account_name: String) -> Result<TOTP, String> {
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some(ISSUER.to_string()),
        account_name,
    )
    .map_err(|e| format!("failed to build totp: {e:?}"))
}

/// Generates a fresh, random RFC 6238 TOTP secret (raw bytes, not yet
/// encrypted). Uses `totp-rs`'s own CSPRNG-backed `Secret::generate_secret`
/// per the task's explicit instruction to generate the secret via the
/// `totp-rs` crate.
pub(crate) fn generate_secret_bytes() -> Vec<u8> {
    Secret::generate_secret()
        .to_bytes()
        .expect("freshly generated totp-rs Secret must convert to raw bytes")
}
