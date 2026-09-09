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
    IdTokenVerifier, OAuthIdentityRepository, PasswordHasher, PasswordResetMailer,
    PasswordResetMailerResult, PendingAuthCache, PendingAuthCacheResult, RateLimiter,
    RateLimiterResult, RegistrationTransaction, ResetPasswordTokenRepository, SecretCipher,
    SecretCipherError, TenantInvitationRepository, TenantMembershipRepository, TenantRepository,
    TokenService, UserRepository,
};
use infra::UuidIdGenerator;

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Default)]
pub struct NoopPendingAuthCache;

#[async_trait]
impl PendingAuthCache for NoopPendingAuthCache {
    async fn get(&self, _key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(None)
    }
    async fn getdel(&self, _key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(None)
    }
    async fn set(&self, _key: &str, _value: &str, _ttl_seconds: u64) -> PendingAuthCacheResult<()> {
        Ok(())
    }
    async fn delete(&self, _key: &str) -> PendingAuthCacheResult<()> {
        Ok(())
    }
}

/// Default OAuth id_token verifier until a real one (e.g.
/// `infra::OidcIdTokenVerifier` or `infra::DisabledIdTokenVerifier`) is
/// wired via `AuthState::with_oauth`. Never accepts a token — a missing
/// verifier must fail closed, not silently authenticate anyone.
#[derive(Clone, Default)]
struct NoopIdTokenVerifier;

#[async_trait]
impl IdTokenVerifier for NoopIdTokenVerifier {
    async fn verify(
        &self,
        _id_token: &str,
    ) -> Result<application::ports::VerifiedIdentity, Box<dyn std::error::Error + Send + Sync>>
    {
        Err("OAuth sign-in is not configured on this AuthState".into())
    }
}

#[derive(Clone, Default)]
struct NoopOAuthIdentityRepository;

#[async_trait]
impl OAuthIdentityRepository for NoopOAuthIdentityRepository {
    async fn find_user_id(&self, _provider: &str, _subject: &str) -> RepoResult<Option<UserId>> {
        Ok(None)
    }
    async fn link(&self, _provider: &str, _subject: &str, _user_id: UserId) -> RepoResult<()> {
        Err("OAuth identity repository is not configured".into())
    }
}

#[derive(Clone, Default)]
struct NoopBackupCodeRepository;

#[async_trait]
impl BackupCodeRepository for NoopBackupCodeRepository {
    async fn create_many(&self, _codes: &[BackupCode]) -> RepoResult<()> {
        Ok(())
    }
    async fn find_all_by_user(&self, _user_id: UserId) -> RepoResult<Vec<BackupCode>> {
        Ok(vec![])
    }
    async fn mark_used(&self, _id: BackupCodeId, _used_at: DateTime<Utc>) -> RepoResult<bool> {
        Ok(true)
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
    async fn find_latest_unused_by_user(
        &self,
        _user_id: UserId,
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
    async fn try_consume(
        &self,
        _id: EmailVerificationTokenId,
        _used_at: DateTime<Utc>,
    ) -> RepoResult<bool> {
        Ok(false)
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
    async fn try_consume(
        &self,
        _id: ResetPasswordTokenId,
        _used_at: DateTime<Utc>,
    ) -> RepoResult<bool> {
        Ok(false)
    }
    async fn delete_unused_for_user(&self, _user_id: UserId) -> RepoResult<()> {
        Ok(())
    }
}

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

/// Default rate limiter used until a real distributed one (e.g.
/// `RedisRateLimiter`) is wired via `AuthState::with_rate_limiter`.
/// Allows every request; intended only for tests or deliberate no-op
/// configurations where rate limiting is handled elsewhere (e.g. by an
/// external proxy).
#[derive(Clone, Default)]
struct NoopRateLimiter;

#[async_trait]
impl RateLimiter for NoopRateLimiter {
    async fn check_and_record(
        &self,
        _namespace: &str,
        _key: &str,
        _max_requests: u32,
        _window_seconds: u64,
    ) -> RateLimiterResult<bool> {
        Ok(true)
    }
}

/// In-memory rate limiter used in API tests where a real Redis is not
/// available. Mirrors the fixed-window semantics of
/// `infra::RedisRateLimiter`, including namespace/key composition,
/// so tests can exercise the exact same handlers' rate limiting logic.
#[derive(Clone, Default)]
pub struct InMemoryRateLimiter {
    store: Arc<tokio::sync::Mutex<HashMap<String, (Instant, u32)>>>,
}

impl InMemoryRateLimiter {
    pub fn new() -> Self {
        Self {
            store: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl RateLimiter for InMemoryRateLimiter {
    async fn check_and_record(
        &self,
        namespace: &str,
        key: &str,
        max_requests: u32,
        window_seconds: u64,
    ) -> RateLimiterResult<bool> {
        if max_requests == 0 || window_seconds == 0 {
            return Ok(false);
        }

        let full_key = format!("rate_limit:{namespace}:{key}");
        let now = Instant::now();
        let mut store = self.store.lock().await;

        let allowed = if let Some((window_start, count)) = store.get_mut(&full_key) {
            if now.duration_since(*window_start).as_secs() >= window_seconds {
                // window expired, start a new one
                *window_start = now;
                *count = 1;
                true
            } else {
                *count += 1;
                *count <= max_requests
            }
        } else {
            store.insert(full_key, (now, 1));
            true
        };

        // Minimal eviction: if the store gets large, drop expired entries.
        if store.len() > 1000 {
            store.retain(|_, (start, _)| now.duration_since(*start).as_secs() < 3600);
        }

        Ok(allowed)
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
    pub rate_limiter: Arc<dyn RateLimiter>,
    /// B6 — Password reset:
    pub reset_password_token_repository: Arc<dyn ResetPasswordTokenRepository>,
    pub password_reset_mailer: Arc<dyn PasswordResetMailer>,
    /// B9 — Tenant invitations:
    pub invitation_repository: Arc<dyn TenantInvitationRepository>,
    /// OAuth — Continue with Google / Apple:
    pub google_id_token_verifier: Arc<dyn IdTokenVerifier>,
    pub apple_id_token_verifier: Arc<dyn IdTokenVerifier>,
    pub oauth_identity_repository: Arc<dyn OAuthIdentityRepository>,
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
            rate_limiter: Arc::new(NoopRateLimiter),
            reset_password_token_repository: Arc::new(NoopResetPasswordTokenRepository),
            password_reset_mailer: Arc::new(NoopPasswordResetMailer),
            invitation_repository: Arc::new(NoopTenantInvitationRepository),
            google_id_token_verifier: Arc::new(NoopIdTokenVerifier),
            apple_id_token_verifier: Arc::new(NoopIdTokenVerifier),
            oauth_identity_repository: Arc::new(NoopOAuthIdentityRepository),
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

    pub fn with_rate_limiter<R>(mut self, rate_limiter: R) -> Self
    where
        R: RateLimiter + 'static,
    {
        self.rate_limiter = Arc::new(rate_limiter);
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

    pub fn with_invitation_repository<IR>(mut self, invitation_repository: IR) -> Self
    where
        IR: TenantInvitationRepository + 'static,
    {
        self.invitation_repository = Arc::new(invitation_repository);
        self
    }

    /// Wires the Google and Apple `id_token` verifiers used by
    /// `POST /auth/oauth/google` and `POST /auth/oauth/apple`. Pass
    /// `infra::DisabledIdTokenVerifier` for a provider that isn't
    /// configured in this deployment (see `main.rs`) rather than
    /// omitting the call — that keeps the failure mode explicit
    /// ("Google sign-in is not configured") instead of falling back to
    /// the default `NoopIdTokenVerifier`'s generic message.
    pub fn with_oauth<G, A, O>(
        mut self,
        google_verifier: G,
        apple_verifier: A,
        identity_repository: O,
    ) -> Self
    where
        G: IdTokenVerifier + 'static,
        A: IdTokenVerifier + 'static,
        O: OAuthIdentityRepository + 'static,
    {
        self.google_id_token_verifier = Arc::new(google_verifier);
        self.apple_id_token_verifier = Arc::new(apple_verifier);
        self.oauth_identity_repository = Arc::new(identity_repository);
        self
    }
}
