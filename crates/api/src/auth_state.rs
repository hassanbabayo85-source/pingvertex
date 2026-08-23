use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    BackupCode, BackupCodeId, EmailVerificationToken, EmailVerificationTokenId, InvitationId,
    ResetPasswordToken, ResetPasswordTokenId, TenantInvitation, UserId,
};

use application::ports::{
    BackupCodeRepository, EmailVerificationMailer, EmailVerificationTokenRepository,
    PasswordHasher, PasswordResetMailer, PasswordResetMailerResult, PendingAuthCache,
    PendingAuthCacheResult, RegistrationTransaction, ResetPasswordTokenRepository, SecretCipher,
    SecretCipherError, TenantInvitationRepository, TenantMembershipRepository, TenantRepository,
    TokenService, UserRepository,
};
use infra::UuidIdGenerator;

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub type ResendVerificationRateLimiter = Arc<tokio::sync::Mutex<HashMap<String, (Instant, u32)>>>;

fn new_resend_verification_rate_limiter() -> ResendVerificationRateLimiter {
    Arc::new(tokio::sync::Mutex::new(HashMap::new()))
}

/// Per-email fixed-window rate limiter for `POST /auth/forgot-password`.
/// Same shape as `ResendVerificationRateLimiter` — see
/// `handlers/forgot_password.rs` for the actual limit values.
pub type ForgotPasswordRateLimiter = Arc<tokio::sync::Mutex<HashMap<String, (Instant, u32)>>>;

fn new_forgot_password_rate_limiter() -> ForgotPasswordRateLimiter {
    Arc::new(tokio::sync::Mutex::new(HashMap::new()))
}

#[derive(Clone, Default)]
pub struct NoopPendingAuthCache;

#[async_trait]
impl PendingAuthCache for NoopPendingAuthCache {
    async fn get(&self, _key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(None)
    }
    async fn set(&self, _key: &str, _value: &str, _ttl_seconds: u64) -> PendingAuthCacheResult<()> {
        Ok(())
    }
    async fn delete(&self, _key: &str) -> PendingAuthCacheResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopBackupCodeRepository;

#[async_trait]
impl BackupCodeRepository for NoopBackupCodeRepository {
    async fn create_many(&self, _codes: &[BackupCode]) -> RepoResult<()> {
        Ok(())
    }
    async fn find_unused_by_user(&self, _user_id: UserId) -> RepoResult<Vec<BackupCode>> {
        Ok(vec![])
    }
    async fn mark_used(&self, _id: BackupCodeId, _used_at: DateTime<Utc>) -> RepoResult<()> {
        Ok(())
    }
    async fn delete_all_for_user(&self, _user_id: UserId) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopEmailVerificationTokenRepository;

#[async_trait]
impl EmailVerificationTokenRepository for NoopEmailVerificationTokenRepository {
    async fn create(&self, _token: &EmailVerificationToken) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_prefix(
        &self,
        _token_prefix: &str,
    ) -> RepoResult<Option<EmailVerificationToken>> {
        Ok(None)
    }
    async fn mark_used(
        &self,
        _id: EmailVerificationTokenId,
        _used_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        Ok(())
    }
    async fn delete_unused_for_user(&self, _user_id: UserId) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopEmailVerificationMailer;

#[async_trait]
impl EmailVerificationMailer for NoopEmailVerificationMailer {
    async fn send_verification_email(
        &self,
        _to: &domain::Email,
        _raw_token: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// Default `ResetPasswordTokenRepository` used until a real one (e.g.
/// `PostgresResetPasswordTokenRepository`, wired in `main.rs`) is
/// substituted in. Mirrors `NoopEmailVerificationTokenRepository`.
#[derive(Clone, Default)]
struct NoopResetPasswordTokenRepository;

#[async_trait]
impl ResetPasswordTokenRepository for NoopResetPasswordTokenRepository {
    async fn create(&self, _token: &ResetPasswordToken) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_prefix(&self, _token_prefix: &str) -> RepoResult<Option<ResetPasswordToken>> {
        Ok(None)
    }
    async fn mark_used(
        &self,
        _id: ResetPasswordTokenId,
        _used_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        Ok(())
    }
    async fn delete_unused_for_user(&self, _user_id: UserId) -> RepoResult<()> {
        Ok(())
    }
}

/// Default `PasswordResetMailer` used until a real one is substituted
/// in. Mirrors `NoopEmailVerificationMailer`.
#[derive(Clone, Default)]
struct NoopPasswordResetMailer;

#[async_trait]
impl PasswordResetMailer for NoopPasswordResetMailer {
    async fn send_password_reset_email(
        &self,
        _to: &domain::Email,
        _raw_token: &str,
    ) -> PasswordResetMailerResult<()> {
        Ok(())
    }
}

/// Default `TenantInvitationRepository` used until a real Postgres one
/// is wired via `AuthState::with_invitation_repository`. Mirrors the
/// other `Noop*` defaults in this file.
#[derive(Clone, Default)]
struct NoopTenantInvitationRepository;

#[async_trait]
impl TenantInvitationRepository for NoopTenantInvitationRepository {
    async fn create(&self, _invitation: &TenantInvitation) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_prefix(&self, _token_prefix: &str) -> RepoResult<Option<TenantInvitation>> {
        Ok(None)
    }
    async fn find_by_id(&self, _id: InvitationId) -> RepoResult<Option<TenantInvitation>> {
        Ok(None)
    }
    async fn list_by_tenant(
        &self,
        _tenant_id: domain::TenantId,
    ) -> RepoResult<Vec<TenantInvitation>> {
        Ok(vec![])
    }
    async fn mark_revoked(&self, _id: InvitationId, _revoked_at: DateTime<Utc>) -> RepoResult<()> {
        Ok(())
    }
    async fn mark_accepted(
        &self,
        _id: InvitationId,
        _accepted_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopSecretCipher;

impl SecretCipher for NoopSecretCipher {
    fn encrypt_bytes(&self, _plaintext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Err(SecretCipherError::EncryptionFailed(
            "secret cipher is not configured on this AuthState".to_string(),
        ))
    }
    fn decrypt_bytes(&self, _ciphertext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Err(SecretCipherError::DecryptionFailed(
            "secret cipher is not configured on this AuthState".to_string(),
        ))
    }
}

#[derive(Clone)]
pub struct AuthState {
    pub user_repository: Arc<dyn UserRepository>,
    pub password_hasher: Arc<dyn PasswordHasher>,
    pub token_service: Arc<dyn TokenService>,
    pub id_generator: UuidIdGenerator,
    pub tenant_repository: Arc<dyn TenantRepository>,
    pub tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
    pub registration_transaction: Arc<dyn RegistrationTransaction>,
    pub secret_cipher: Arc<dyn SecretCipher>,
    pub pending_auth_cache: Arc<dyn PendingAuthCache>,
    pub backup_code_repository: Arc<dyn BackupCodeRepository>,
    pub email_verification_token_repository: Arc<dyn EmailVerificationTokenRepository>,
    pub email_verification_mailer: Arc<dyn EmailVerificationMailer>,
    pub resend_verification_rate_limiter: ResendVerificationRateLimiter,
    /// B6 — Password reset:
    pub reset_password_token_repository: Arc<dyn ResetPasswordTokenRepository>,
    pub password_reset_mailer: Arc<dyn PasswordResetMailer>,
    pub forgot_password_rate_limiter: ForgotPasswordRateLimiter,
    /// B9 — Tenant invitations:
    pub invitation_repository: Arc<dyn TenantInvitationRepository>,
}

impl AuthState {
    pub fn new<U, H, T, TR, TM, REG>(
        user_repository: U,
        password_hasher: H,
        token_service: T,
        id_generator: UuidIdGenerator,
        tenant_repository: TR,
        tenant_membership_repository: TM,
        registration_transaction: REG,
    ) -> Self
    where
        U: UserRepository + 'static,
        H: PasswordHasher + 'static,
        T: TokenService + 'static,
        TR: TenantRepository + 'static,
        TM: TenantMembershipRepository + 'static,
        REG: RegistrationTransaction + 'static,
    {
        Self {
            user_repository: Arc::new(user_repository),
            password_hasher: Arc::new(password_hasher),
            token_service: Arc::new(token_service),
            id_generator,
            tenant_repository: Arc::new(tenant_repository),
            tenant_membership_repository: Arc::new(tenant_membership_repository),
            registration_transaction: Arc::new(registration_transaction),
            secret_cipher: Arc::new(NoopSecretCipher),
            pending_auth_cache: Arc::new(NoopPendingAuthCache),
            backup_code_repository: Arc::new(NoopBackupCodeRepository),
            email_verification_token_repository: Arc::new(NoopEmailVerificationTokenRepository),
            email_verification_mailer: Arc::new(NoopEmailVerificationMailer),
            resend_verification_rate_limiter: new_resend_verification_rate_limiter(),
            reset_password_token_repository: Arc::new(NoopResetPasswordTokenRepository),
            password_reset_mailer: Arc::new(NoopPasswordResetMailer),
            forgot_password_rate_limiter: new_forgot_password_rate_limiter(),
            invitation_repository: Arc::new(NoopTenantInvitationRepository),
        }
    }

    pub fn with_totp_support<S, C, B>(
        mut self,
        secret_cipher: S,
        pending_auth_cache: C,
        backup_code_repository: B,
    ) -> Self
    where
        S: SecretCipher + 'static,
        C: PendingAuthCache + 'static,
        B: BackupCodeRepository + 'static,
    {
        self.secret_cipher = Arc::new(secret_cipher);
        self.pending_auth_cache = Arc::new(pending_auth_cache);
        self.backup_code_repository = Arc::new(backup_code_repository);
        self
    }

    pub fn with_email_verification<R, M>(mut self, token_repository: R, mailer: M) -> Self
    where
        R: EmailVerificationTokenRepository + 'static,
        M: EmailVerificationMailer + 'static,
    {
        self.email_verification_token_repository = Arc::new(token_repository);
        self.email_verification_mailer = Arc::new(mailer);
        self
    }

    pub fn with_password_reset<R, M>(mut self, token_repository: R, mailer: M) -> Self
    where
        R: ResetPasswordTokenRepository + 'static,
        M: PasswordResetMailer + 'static,
    {
        self.reset_password_token_repository = Arc::new(token_repository);
        self.password_reset_mailer = Arc::new(mailer);
        self
    }

    /// Substitutes a real B9 invitation repository in place of the
    /// `NoopTenantInvitationRepository` default.
    pub fn with_invitation_repository<IR>(mut self, invitation_repository: IR) -> Self
    where
        IR: TenantInvitationRepository + 'static,
    {
        self.invitation_repository = Arc::new(invitation_repository);
        self
    }
}
