use std::error::Error;

use async_trait::async_trait;

/// Abstraction for password hashing.
///
/// The application layer never knows which algorithm
/// is used (bcrypt, argon2, scrypt, etc.).
#[async_trait]
pub trait PasswordHasher: Send + Sync {
    async fn hash(&self, password: &str) -> Result<String, Box<dyn Error + Send + Sync>>;

    async fn verify(
        &self,
        password: &str,
        hash: &str,
    ) -> Result<bool, Box<dyn Error + Send + Sync>>;
}

use std::sync::Arc;

#[async_trait]
impl<T> PasswordHasher for Arc<T>
where
    T: PasswordHasher + ?Sized,
{
    async fn hash(
        &self,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        (**self).hash(password).await
    }
    async fn verify(
        &self,
        password: &str,
        hash: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        (**self).verify(password, hash).await
    }
}
