use application::ports::IdGenerator;
use async_trait::async_trait;
use domain::{ApiFlowId, CheckId, IncidentId, MonitorId, NotificationChannelId, UserId};
use rand_core::{OsRng, RngCore};

/// Number of random bytes used for the plaintext lookup prefix of a raw
/// API key. 8 bytes = 16 hex chars — enough entropy that prefixes won't
/// collide in practice, while staying short and readable.
const API_KEY_PREFIX_BYTES: usize = 8;
/// Number of random bytes used for the secret portion of a raw API key.
/// 32 bytes (256 bits) matches `next_webhook_signing_secret`'s secret
/// size below.
const API_KEY_SECRET_BYTES: usize = 32;

/// Number of random bytes backing each backup code, hex-encoded into a
/// 16-character code (grouped as `xxxx-xxxx-xxxx-xxxx` for readability).
/// 8 bytes (64 bits) of entropy is far beyond brute-forceable given these
/// are also rate-limited and Argon2id-hashed at rest.
const BACKUP_CODE_BYTES: usize = 8;

/// Number of random bytes backing the opaque pending-2FA-login token.
/// 32 bytes (256 bits), same size as a webhook signing secret — this
/// token is a bearer credential (whoever holds it and the right TOTP/
/// backup code gets a session), so it needs the same entropy budget.
const PENDING_TWO_FACTOR_TOKEN_BYTES: usize = 32;

/// Number of random bytes used for the plaintext lookup prefix of a raw
/// email-verification token. 8 bytes = 16 hex chars, same rationale as
/// API key prefixes.
const EMAIL_VERIFICATION_PREFIX_BYTES: usize = 8;
/// Number of random bytes used for the secret portion of a raw
/// email-verification token. 32 bytes (256 bits).
const EMAIL_VERIFICATION_SECRET_BYTES: usize = 32;

/// Number of random bytes used for the plaintext lookup prefix of a raw
/// password-reset token. Same size/rationale as
/// `EMAIL_VERIFICATION_PREFIX_BYTES` — the two token kinds share an
/// identical entropy budget, only the scheme tag differs.
const RESET_PASSWORD_PREFIX_BYTES: usize = 8;
/// Number of random bytes used for the secret portion of a raw
/// password-reset token.
const RESET_PASSWORD_SECRET_BYTES: usize = 32;

/// Number of random bytes used for the plaintext lookup prefix of a raw
/// tenant-invitation token. Same size/rationale as the other token kinds.
const INVITATION_PREFIX_BYTES: usize = 8;
/// Number of random bytes used for the secret portion of a raw
/// tenant-invitation token.
const INVITATION_SECRET_BYTES: usize = 32;

#[derive(Debug, Default, Clone)]
pub struct UuidIdGenerator;

impl UuidIdGenerator {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl IdGenerator for UuidIdGenerator {
    async fn next_user_id(&self) -> UserId {
        UserId::generate()
    }

    async fn next_monitor_id(&self) -> MonitorId {
        MonitorId::generate()
    }

    async fn next_check_id(&self) -> CheckId {
        CheckId::generate()
    }

    async fn next_incident_id(&self) -> IncidentId {
        IncidentId::generate()
    }

    async fn next_notification_channel_id(&self) -> NotificationChannelId {
        NotificationChannelId::generate()
    }

    async fn next_api_flow_id(&self) -> ApiFlowId {
        ApiFlowId::new()
    }

    async fn next_webhook_signing_secret(&self) -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        format!("whsec_{}", hex::encode(bytes))
    }

    async fn next_api_key_secret(&self) -> String {
        let mut prefix_bytes = [0u8; API_KEY_PREFIX_BYTES];
        OsRng.fill_bytes(&mut prefix_bytes);

        let mut secret_bytes = [0u8; API_KEY_SECRET_BYTES];
        OsRng.fill_bytes(&mut secret_bytes);

        format!(
            "pv_{}_{}",
            hex::encode(prefix_bytes),
            hex::encode(secret_bytes)
        )
    }

    async fn next_backup_code(&self) -> String {
        let mut bytes = [0u8; BACKUP_CODE_BYTES];
        OsRng.fill_bytes(&mut bytes);
        let hex = hex::encode(bytes);
        hex.as_bytes()
            .chunks(4)
            .map(|chunk| std::str::from_utf8(chunk).expect("hex output is valid utf8"))
            .collect::<Vec<_>>()
            .join("-")
    }

    async fn next_pending_two_factor_token(&self) -> String {
        let mut bytes = [0u8; PENDING_TWO_FACTOR_TOKEN_BYTES];
        OsRng.fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    async fn next_email_verification_token(&self) -> String {
        let mut prefix_bytes = [0u8; EMAIL_VERIFICATION_PREFIX_BYTES];
        OsRng.fill_bytes(&mut prefix_bytes);

        let mut secret_bytes = [0u8; EMAIL_VERIFICATION_SECRET_BYTES];
        OsRng.fill_bytes(&mut secret_bytes);

        format!(
            "evt_{}_{}",
            hex::encode(prefix_bytes),
            hex::encode(secret_bytes)
        )
    }

    async fn next_email_verification_otp(&self) -> String {
        // Rejection sampling avoids modulo bias while preserving a uniform
        // distribution over all one-million 6-digit codes.
        const OTP_SPACE: u32 = 1_000_000;
        const ACCEPT_LIMIT: u32 = u32::MAX - (u32::MAX % OTP_SPACE);

        loop {
            let mut bytes = [0u8; 4];
            OsRng.fill_bytes(&mut bytes);
            let value = u32::from_be_bytes(bytes);
            if value < ACCEPT_LIMIT {
                return format!("{value:06}", value = value % OTP_SPACE);
            }
        }
    }

    async fn next_reset_password_token(&self) -> String {
        let mut prefix_bytes = [0u8; RESET_PASSWORD_PREFIX_BYTES];
        OsRng.fill_bytes(&mut prefix_bytes);

        let mut secret_bytes = [0u8; RESET_PASSWORD_SECRET_BYTES];
        OsRng.fill_bytes(&mut secret_bytes);

        format!(
            "rst_{}_{}",
            hex::encode(prefix_bytes),
            hex::encode(secret_bytes)
        )
    }

    async fn next_invitation_token(&self) -> String {
        let mut prefix_bytes = [0u8; INVITATION_PREFIX_BYTES];
        OsRng.fill_bytes(&mut prefix_bytes);

        let mut secret_bytes = [0u8; INVITATION_SECRET_BYTES];
        OsRng.fill_bytes(&mut secret_bytes);

        format!(
            "inv_{}_{}",
            hex::encode(prefix_bytes),
            hex::encode(secret_bytes)
        )
    }
}
