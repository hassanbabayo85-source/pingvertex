use std::error::Error;

use application::ports::TokenService;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use uuid::Uuid;

use crate::services::Claims;
use domain::UserId;

#[derive(Debug, Clone)]
pub struct JsonWebTokenService {
    secret: String,
}

impl JsonWebTokenService {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }
}

#[async_trait]
impl TokenService for JsonWebTokenService {
    async fn generate(&self, user_id: UserId) -> Result<String, Box<dyn Error + Send + Sync>> {
        let claims = Claims {
            sub: user_id.to_string(),
            exp: (Utc::now() + Duration::hours(24)).timestamp() as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| format!("jwt encode error: {e}").into())
    }

    async fn verify(&self, token: &str) -> Result<UserId, Box<dyn Error + Send + Sync>> {
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| format!("jwt decode error: {e}"))?;

        let uuid =
            Uuid::parse_str(&data.claims.sub).map_err(|e| format!("invalid user id: {e}"))?;

        Ok(UserId::from_uuid(uuid))
    }
}
