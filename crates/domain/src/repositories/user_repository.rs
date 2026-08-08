use async_trait::async_trait;

use crate::{Email, User, UserId, Username};

#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Persist a newly created user.
    async fn create(&self, user: &User) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Update an existing user.
    async fn update(&self, user: &User) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Find by unique identifier.
    async fn find_by_id(
        &self,
        id: &UserId,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;

    /// Find by email.
    async fn find_by_email(
        &self,
        email: &Email,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;

    /// Find by username.
    async fn find_by_username(
        &self,
        username: &Username,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>>;

    /// Permanently remove a user.
    async fn delete(&self, id: &UserId) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Check email uniqueness.
    async fn email_exists(
        &self,
        email: &Email,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;

    /// Check username uniqueness.
    async fn username_exists(
        &self,
        username: &Username,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;
}
