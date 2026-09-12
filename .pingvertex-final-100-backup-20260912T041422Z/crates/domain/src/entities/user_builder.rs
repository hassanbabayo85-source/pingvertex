use chrono::{DateTime, Utc};

use crate::{Email, PasswordHash, User, UserId, UserRole, UserStatus, Username};

#[derive(Debug, Default)]
pub struct UserBuilder {
    id: Option<UserId>,
    username: Option<Username>,
    email: Option<Email>,
    password_hash: Option<PasswordHash>,
    role: UserRole,
    status: UserStatus,
    email_verified_at: Option<DateTime<Utc>>,
    totp_secret: Option<Vec<u8>>,
    totp_enabled: bool,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

impl UserBuilder {
    pub fn new() -> Self {
        Self {
            role: UserRole::default(),
            status: UserStatus::default(),
            ..Default::default()
        }
    }

    pub fn id(mut self, value: UserId) -> Self {
        self.id = Some(value);
        self
    }

    pub fn username(mut self, value: Username) -> Self {
        self.username = Some(value);
        self
    }

    pub fn email(mut self, value: Email) -> Self {
        self.email = Some(value);
        self
    }

    pub fn password_hash(mut self, value: PasswordHash) -> Self {
        self.password_hash = Some(value);
        self
    }

    pub fn role(mut self, value: UserRole) -> Self {
        self.role = value;
        self
    }

    pub fn status(mut self, value: UserStatus) -> Self {
        self.status = value;
        self
    }

    pub fn email_verified_at(mut self, value: Option<DateTime<Utc>>) -> Self {
        self.email_verified_at = value;
        self
    }

    /// Defaults to `None`, matching every user's state before 2FA
    /// enrollment — most callers (registration, existing tests) never
    /// need to set this explicitly.
    pub fn totp_secret(mut self, value: Option<Vec<u8>>) -> Self {
        self.totp_secret = value;
        self
    }

    /// Defaults to `false` (see `Default` derive on this struct's field),
    /// matching every user's state before 2FA enrollment.
    pub fn totp_enabled(mut self, value: bool) -> Self {
        self.totp_enabled = value;
        self
    }

    pub fn created_at(mut self, value: DateTime<Utc>) -> Self {
        self.created_at = Some(value);
        self
    }

    pub fn updated_at(mut self, value: DateTime<Utc>) -> Self {
        self.updated_at = Some(value);
        self
    }

    pub fn build(self) -> Result<User, &'static str> {
        Ok(User::new(
            self.id.ok_or("missing id")?,
            self.username.ok_or("missing username")?,
            self.email.ok_or("missing email")?,
            self.password_hash.ok_or("missing password_hash")?,
            self.role,
            self.status,
            self.email_verified_at,
            self.totp_secret,
            self.totp_enabled,
            self.created_at.ok_or("missing created_at")?,
            self.updated_at.ok_or("missing updated_at")?,
        ))
    }
}
