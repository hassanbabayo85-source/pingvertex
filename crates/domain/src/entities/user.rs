use chrono::{DateTime, Utc};

use crate::{Email, PasswordHash, UserId, UserRole, UserStatus, Username};

#[derive(Clone)]
pub struct User {
    id: UserId,
    username: Username,
    email: Email,
    password_hash: PasswordHash,

    role: UserRole,
    status: UserStatus,

    email_verified_at: Option<DateTime<Utc>>,

    /// Encrypted (via `application::ports::SecretCipher`) TOTP shared
    /// secret, `None` until the user has completed 2FA enrollment
    /// (`ConfirmTotpSetupUseCase`). Mirrors the
    /// `NotificationChannel.encrypted_secret` pattern: the secret is
    /// never stored in plaintext at rest.
    totp_secret: Option<Vec<u8>>,
    /// `true` once TOTP 2FA has been confirmed and turned on for this
    /// account. While `true`, `LoginUserUseCase` no longer issues a
    /// session JWT directly — it returns a short-lived pending-2FA token
    /// instead, and the caller must complete `VerifyTotpAndLoginUseCase`.
    totp_enabled: bool,

    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl User {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: UserId,
        username: Username,
        email: Email,
        password_hash: PasswordHash,
        role: UserRole,
        status: UserStatus,
        email_verified_at: Option<DateTime<Utc>>,
        totp_secret: Option<Vec<u8>>,
        totp_enabled: bool,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            username,
            email,
            password_hash,
            role,
            status,
            email_verified_at,
            totp_secret,
            totp_enabled,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> UserId {
        self.id
    }

    pub fn username(&self) -> &Username {
        &self.username
    }

    pub fn email(&self) -> &Email {
        &self.email
    }

    pub fn password_hash(&self) -> &PasswordHash {
        &self.password_hash
    }

    pub fn role(&self) -> UserRole {
        self.role
    }

    pub fn status(&self) -> UserStatus {
        self.status
    }

    pub fn email_verified_at(&self) -> Option<DateTime<Utc>> {
        self.email_verified_at
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn is_email_verified(&self) -> bool {
        self.email_verified_at.is_some()
    }

    /// Encrypted TOTP secret, if 2FA has ever been enrolled for this user.
    /// Callers must decrypt via `SecretCipher::decrypt_bytes` before
    /// passing to `totp_rs::TOTP` — never persisted or compared in
    /// plaintext.
    pub fn totp_secret(&self) -> Option<&Vec<u8>> {
        self.totp_secret.as_ref()
    }

    pub fn totp_enabled(&self) -> bool {
        self.totp_enabled
    }

    /// Turns on TOTP 2FA for this account, storing the (already
    /// encrypted, via `SecretCipher`) secret. Called by
    /// `ConfirmTotpSetupUseCase` once the user has proven possession of
    /// the secret by submitting a valid first code.
    pub fn enable_totp(&mut self, encrypted_secret: Vec<u8>, when: DateTime<Utc>) {
        self.totp_secret = Some(encrypted_secret);
        self.totp_enabled = true;
        self.updated_at = when;
    }

    /// Turns off TOTP 2FA and discards the stored secret. Backup codes
    /// tied to this account are the caller's (use case's) responsibility
    /// to invalidate separately — this method only touches the `User`
    /// aggregate's own fields.
    pub fn disable_totp(&mut self, when: DateTime<Utc>) {
        self.totp_secret = None;
        self.totp_enabled = false;
        self.updated_at = when;
    }

    pub fn mark_email_verified(&mut self, when: DateTime<Utc>) {
        self.email_verified_at = Some(when);
        self.updated_at = when;
        self.status = UserStatus::Active;
    }

    /// Replaces this user's password hash — the sole mutation performed
    /// by a successful `ResetPasswordUseCase` redemption. Does not
    /// touch any other field (role/status/TOTP/etc are left exactly as
    /// they were); callers are responsible for persisting the change via
    /// `UserRepository::update` and for handling session invalidation
    /// separately (see `application::session_invalidated_at_cache_key`).
    pub fn set_password_hash(&mut self, password_hash: PasswordHash, when: DateTime<Utc>) {
        self.password_hash = password_hash;
        self.updated_at = when;
    }

    pub fn suspend(&mut self, when: DateTime<Utc>) {
        self.status = UserStatus::Suspended;
        self.updated_at = when;
    }

    pub fn activate(&mut self, when: DateTime<Utc>) {
        self.status = UserStatus::Active;
        self.updated_at = when;
    }

    pub fn delete(&mut self, when: DateTime<Utc>) {
        self.status = UserStatus::Deleted;
        self.updated_at = when;
    }
}

/// Manual Debug that never prints `password_hash` or the
/// (encrypted) `totp_secret`. Derived `Debug` would have leaked both.
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("role", &self.role)
            .field("status", &self.status)
            .field("email_verified", &self.email_verified_at.is_some())
            .field("totp_enabled", &self.totp_enabled)
            .field("password_hash", &"[REDACTED]")
            .field(
                "totp_secret",
                &self.totp_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}
