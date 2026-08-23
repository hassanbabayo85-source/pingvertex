use std::error::Error;

use chrono::Utc;

use domain::{DomainError, UserId};

use crate::{
    dto::DisableTotpRequest,
    ports::{BackupCodeRepository, PasswordHasher, UserRepository},
};

type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Turns off TOTP 2FA for an account. Requires the caller to re-submit
/// their current password (step-up re-authentication) — a valid session
/// JWT alone only proves "someone with this cookie/header", not "the
/// account owner, right now", and disabling 2FA is exactly as sensitive
/// as changing the password itself.
pub struct DisableTotpUseCase<R, H, B> {
    repository: R,
    password_hasher: H,
    backup_code_repository: B,
}

impl<R, H, B> DisableTotpUseCase<R, H, B>
where
    R: UserRepository,
    H: PasswordHasher,
    B: BackupCodeRepository,
{
    pub fn new(repository: R, password_hasher: H, backup_code_repository: B) -> Self {
        Self {
            repository,
            password_hasher,
            backup_code_repository,
        }
    }

    pub async fn execute(&self, user_id: UserId, request: DisableTotpRequest) -> UseCaseResult<()> {
        let mut user = self
            .repository
            .find_by_id(&user_id)
            .await?
            .ok_or("user not found")?;

        if !user.totp_enabled() {
            return Err(DomainError::TotpNotEnabled.into());
        }

        let valid = self
            .password_hasher
            .verify(&request.password, user.password_hash().as_str())
            .await
            .unwrap_or(false);

        if !valid {
            return Err("invalid password".into());
        }

        user.disable_totp(Utc::now());
        self.repository.update(&user).await?;
        self.backup_code_repository
            .delete_all_for_user(user_id)
            .await?;

        Ok(())
    }
}
