use std::error::Error;

use chrono::Utc;
use uuid::Uuid;

use domain::{DomainError, UserId};

use crate::{
    dto::{AuthResponse, VerifyTotpRequest},
    ports::{
        BackupCodeRepository, PasswordHasher, PendingAuthCache, SecretCipher, TokenService,
        UserRepository,
    },
    use_cases::totp_support::{build_totp, pending_login_cache_key},
};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Completes a 2FA-gated login: redeems the `pending_token` issued by
/// `LoginUserUseCase` together with either a 6-digit TOTP code or a
/// backup code, and — if valid — calls `token_service.generate(user.id())`
/// exactly the way `login_user.rs` does for a non-2FA login.
pub struct VerifyTotpAndLoginUseCase<R, S, H, T, C, B> {
    repository: R,
    secret_cipher: S,
    password_hasher: H,
    token_service: T,
    pending_auth_cache: C,
    backup_code_repository: B,
}

impl<R, S, H, T, C, B> VerifyTotpAndLoginUseCase<R, S, H, T, C, B>
where
    R: UserRepository,
    S: SecretCipher,
    H: PasswordHasher,
    T: TokenService,
    C: PendingAuthCache,
    B: BackupCodeRepository,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: R,
        secret_cipher: S,
        password_hasher: H,
        token_service: T,
        pending_auth_cache: C,
        backup_code_repository: B,
    ) -> Self {
        Self {
            repository,
            secret_cipher,
            password_hasher,
            token_service,
            pending_auth_cache,
            backup_code_repository,
        }
    }

    pub async fn execute(&self, request: VerifyTotpRequest) -> UseCaseResult<AuthResponse> {
        let cache_key = pending_login_cache_key(&request.pending_token);

        // Atomic get-and-delete: the pending 2FA token is single-use.
        // The first request that executes this line consumes the token;
        // any concurrent request with the same pending_token now gets
        // `None` and fails with `InvalidPendingTwoFactorToken`.
        let user_id_str = self
            .pending_auth_cache
            .getdel(&cache_key)
            .await
            .map_err(|e| format!("failed to consume pending 2FA token: {e}"))?
            .ok_or(DomainError::InvalidPendingTwoFactorToken)?;

        let user_id = UserId(
            Uuid::parse_str(&user_id_str).map_err(|_| DomainError::InvalidPendingTwoFactorToken)?,
        );

        let user = self
            .repository
            .find_by_id(&user_id)
            .await?
            .ok_or(DomainError::InvalidPendingTwoFactorToken)?;

        // The account's 2FA state may have changed between login and
        // verify (e.g. concurrently disabled) — fail closed rather than
        // silently granting a session.
        if !user.totp_enabled() {
            return Err(DomainError::InvalidPendingTwoFactorToken.into());
        }

        let mut verified = false;

        // Try TOTP first (cheap, no DB round-trip). `build_totp`'s skew
        // of 1 covers the required ±1 step clock-skew tolerance.
        if let Some(encrypted_secret) = user.totp_secret()
            && let Ok(secret_bytes) = self.secret_cipher.decrypt_bytes(encrypted_secret)
            && let Ok(totp) = build_totp(secret_bytes, user.email().as_str().to_string())
            && let Ok(true) = totp.check_current(&request.code)
        {
            verified = true;
        }

        // Fall back to a backup code only when TOTP didn't match.
        if !verified {
            let all_codes = self
                .backup_code_repository
                .find_all_by_user(user_id)
                .await?;

            for candidate in all_codes {
                let matches = self
                    .password_hasher
                    .verify(&request.code, &candidate.code_hash)
                    .await
                    .unwrap_or(false);

                if matches {
                    // If this code was already used, it's an invalid backup
                    // code (single-use). If it's still unused, atomically
                    // try to redeem it; a false return means a concurrent
                    // request won the race.
                    if candidate.used_at.is_some() {
                        return Err(DomainError::InvalidBackupCode.into());
                    }

                    let redeemed = self
                        .backup_code_repository
                        .mark_used(candidate.id, Utc::now())
                        .await?;

                    if redeemed {
                        verified = true;
                    } else {
                        return Err(DomainError::InvalidBackupCode.into());
                    }
                    break;
                }
            }
        }

        if !verified {
            return Err(DomainError::InvalidTotpCode.into());
        }

        let token = self.token_service.generate(user.id()).await?;

        Ok(AuthResponse {
            access_token: token,
        })
    }
}
