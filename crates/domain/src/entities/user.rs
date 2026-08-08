use chrono::{DateTime, Utc};

use crate::{Email, PasswordHash, UserId, UserRole, UserStatus, Username};

#[derive(Debug, Clone)]
pub struct User {
    id: UserId,
    username: Username,
    email: Email,
    password_hash: PasswordHash,

    role: UserRole,
    status: UserStatus,

    email_verified_at: Option<DateTime<Utc>>,

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

    pub fn mark_email_verified(&mut self, when: DateTime<Utc>) {
        self.email_verified_at = Some(when);
        self.updated_at = when;
        self.status = UserStatus::Active;
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
