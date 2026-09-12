use std::error::Error;

use application::ports::PasswordHasher;
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString},
};
use async_trait::async_trait;
use rand_core::OsRng;

#[derive(Debug, Clone, Default)]
pub struct Argon2PasswordHasher;

#[async_trait]
impl PasswordHasher for Argon2PasswordHasher {
    async fn hash(&self, password: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
        let salt = SaltString::generate(&mut OsRng);

        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| format!("argon2 hash error: {e}"))?
            .to_string();

        Ok(hash)
    }

    async fn verify(
        &self,
        password: &str,
        hash: &str,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let parsed = PasswordHash::new(hash).map_err(|e| format!("invalid password hash: {e}"))?;

        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }
}
