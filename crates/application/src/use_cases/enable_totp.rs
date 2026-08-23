use std::error::Error;

use base64::{Engine, engine::general_purpose::STANDARD};

use domain::{DomainError, UserId};

use crate::{
    dto::EnableTotpResponse,
    ports::{PendingAuthCache, SecretCipher, UserRepository},
    use_cases::totp_support::{
        PENDING_SECRET_TTL_SECONDS, build_totp, generate_secret_bytes, pending_secret_cache_key,
    },
};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Starts TOTP 2FA enrollment for an already-authenticated user (called
/// behind `jwt_auth` — see `crates/api/src/routes/auth.rs`).
///
/// Generates a fresh secret, returns an `otpauth://` URI for the client
/// to render as a QR code, and stashes the ENCRYPTED secret in
/// `PendingAuthCache` (Redis) for up to 10 minutes. `user.totp_enabled`
/// is deliberately NOT flipped on here — only `ConfirmTotpSetupUseCase`
/// does that, once the user proves possession of the secret by
/// submitting a valid first code.
pub struct EnableTotpUseCase<R, S, C> {
    repository: R,
    secret_cipher: S,
    pending_auth_cache: C,
}

impl<R, S, C> EnableTotpUseCase<R, S, C>
where
    R: UserRepository,
    S: SecretCipher,
    C: PendingAuthCache,
{
    pub fn new(repository: R, secret_cipher: S, pending_auth_cache: C) -> Self {
        Self {
            repository,
            secret_cipher,
            pending_auth_cache,
        }
    }

    pub async fn execute(&self, user_id: UserId) -> UseCaseResult<EnableTotpResponse> {
        let user = self
            .repository
            .find_by_id(&user_id)
            .await?
            .ok_or("user not found")?;

        if user.totp_enabled() {
            return Err(DomainError::TotpAlreadyEnabled.into());
        }

        let secret_bytes = generate_secret_bytes();

        let totp = build_totp(secret_bytes.clone(), user.email().as_str().to_string())?;
        let otpauth_uri = totp.get_url();

        let encrypted = self
            .secret_cipher
            .encrypt_bytes(&secret_bytes)
            .map_err(|e| format!("failed to encrypt totp secret: {e}"))?;

        // Cached as base64 text since `PendingAuthCache` (like
        // `StatusPageCache`) stores/returns `String` values, not raw
        // bytes.
        let encoded = STANDARD.encode(encrypted);

        self.pending_auth_cache
            .set(
                &pending_secret_cache_key(user_id),
                &encoded,
                PENDING_SECRET_TTL_SECONDS,
            )
            .await
            .map_err(|e| format!("failed to store pending totp secret: {e}"))?;

        Ok(EnableTotpResponse { otpauth_uri })
    }
}
