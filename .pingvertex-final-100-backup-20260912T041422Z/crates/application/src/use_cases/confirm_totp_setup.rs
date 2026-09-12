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
    use_cases::totp_support::{build_totp, pending_secret_cache_key},
};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Ten single-use recovery codes are generated on every successful
/// enrollment, per the task's requirements.
const BACKUP_CODE_COUNT: usize = 10;

/// Finishes TOTP 2FA enrollment: verifies the user's first code against
/// the pending secret stashed by `EnableTotpUseCase`, and if valid,
/// permanently enables 2FA on the account and issues fresh backup codes.
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

        // Persist the SAME ciphertext that was sitting in the pending
        // cache — it already came from `SecretCipher::encrypt_bytes` in
        // `EnableTotpUseCase`, nothing to re-derive.
        user.enable_totp(encrypted, now);
        self.repository.update(&user).await?;

        // Any codes from a previous, since-disabled enrollment must not
        // remain redeemable once a new secret takes over.
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

        Ok(ConfirmTotpSetupResponse {
            backup_codes: raw_codes,
        })
    }
}
