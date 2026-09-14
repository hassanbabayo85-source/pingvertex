use std::error::Error;

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;

use domain::{BackupCode, DomainError, UserId};

use crate::{
    dto::{ConfirmTotpSetupRequest, ConfirmTotpSetupResponse},
    ports::{
        BackupCodeRepository, IdGenerator, PasswordHasher, PendingAuthCache, SecretCipher,
        UserRepository,
    },
    use_cases::{
        recent_mfa::{RECENT_MFA_TTL_SECONDS, recent_mfa_cache_key},
        totp_support::{build_totp, pending_secret_cache_key},
    },
};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const BACKUP_CODE_COUNT: usize = 10;

pub struct ConfirmTotpSetupUseCase<R, S, H, C, B, G> {
    repository: R,
    secret_cipher: S,
    password_hasher: H,
    pending_auth_cache: C,
    backup_code_repository: B,
    id_generator: G,
}

impl<R, S, H, C, B, G> ConfirmTotpSetupUseCase<R, S, H, C, B, G>
where
    R: UserRepository,
    S: SecretCipher,
    H: PasswordHasher,
    C: PendingAuthCache,
    B: BackupCodeRepository,
    G: IdGenerator,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: R,
        secret_cipher: S,
        password_hasher: H,
        pending_auth_cache: C,
        backup_code_repository: B,
        id_generator: G,
    ) -> Self {
        Self {
            repository,
            secret_cipher,
            password_hasher,
            pending_auth_cache,
            backup_code_repository,
            id_generator,
        }
    }

    pub async fn execute(
        &self,
        user_id: UserId,
        request: ConfirmTotpSetupRequest,
    ) -> UseCaseResult<ConfirmTotpSetupResponse> {
        let mut user = self
            .repository
            .find_by_id(&user_id)
            .await?
            .ok_or("user not found")?;

        if user.totp_enabled() {
            return Err(DomainError::TotpAlreadyEnabled.into());
        }

        let cache_key = pending_secret_cache_key(user_id);

        let encoded = self
            .pending_auth_cache
            .get(&cache_key)
            .await
            .map_err(|e| format!("failed to read pending totp secret: {e}"))?
            .ok_or(DomainError::NoPendingTotpSetup)?;

        let encrypted = STANDARD
            .decode(encoded)
            .map_err(|_| "corrupt pending totp secret")?;

        let secret_bytes = self
            .secret_cipher
            .decrypt_bytes(&encrypted)
            .map_err(|e| format!("failed to decrypt totp secret: {e}"))?;

        let totp = build_totp(secret_bytes, user.email().as_str().to_string())?;

        let valid = totp
            .check_current(&request.totp_code)
            .map_err(|e| format!("failed to check totp code: {e}"))?;

        if !valid {
            return Err(DomainError::InvalidTotpCode.into());
        }

        let now = Utc::now();

        user.enable_totp(encrypted, now);
        self.repository.update(&user).await?;

        self.backup_code_repository
            .delete_all_for_user(user_id)
            .await?;

        let mut raw_codes = Vec::with_capacity(BACKUP_CODE_COUNT);
        let mut codes = Vec::with_capacity(BACKUP_CODE_COUNT);

        for _ in 0..BACKUP_CODE_COUNT {
            let raw_code = self.id_generator.next_backup_code().await;

            let code_hash = self
                .password_hasher
                .hash(&raw_code)
                .await
                .map_err(|e| format!("failed to hash backup code: {e}"))?;

            let id = self.id_generator.next_backup_code_id().await;
            codes.push(BackupCode::new(id, user_id, code_hash, now));
            raw_codes.push(raw_code);
        }

        self.backup_code_repository.create_many(&codes).await?;

        self.pending_auth_cache
            .delete(&cache_key)
            .await
            .map_err(|e| format!("failed to clear pending totp secret: {e}"))?;

        // SEC: enrolling in 2FA also counts as "recent MFA" for the
        // step-up window. The user just proved possession of their
        // second factor (they submitted a valid first code), so
        // blocking them from high-risk admin operations for the next
        // few minutes would be pure friction with no security benefit.
        // Mirrors the marker written by `VerifyTotpAndLoginUseCase`.
        self.pending_auth_cache
            .set(
                &recent_mfa_cache_key(user_id),
                &now.timestamp().to_string(),
                RECENT_MFA_TTL_SECONDS,
            )
            .await
            .map_err(|e| format!("failed to record recent-mfa marker: {e}"))?;

        Ok(ConfirmTotpSetupResponse {
            backup_codes: raw_codes,
        })
    }
}
