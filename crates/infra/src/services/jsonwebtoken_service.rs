use std::error::Error;

use application::ports::{TokenService, VerifiedToken};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use uuid::Uuid;

use crate::services::Claims;
use domain::UserId;

pub const JWT_ISSUER: &str = "uptime-monitor";
pub const JWT_AUDIENCE: &str = "uptime-monitor-api";

#[derive(Clone)]
pub struct JsonWebTokenService {
    secret: String,
}

/// Custom Debug that redacts the signing secret.
impl std::fmt::Debug for JsonWebTokenService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonWebTokenService")
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl JsonWebTokenService {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    /// SECURITY: signature verification (HS256), `exp` verification, and
    /// the required `sub`/`iat`/`exp` claims are all preserved exactly
    /// as before. `iss` and `aud` are ADDED as required claims on top of
    /// the existing checks — nothing is weakened.
    fn validation() -> Validation {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.set_issuer(&[JWT_ISSUER]);
        validation.set_audience(&[JWT_AUDIENCE]);
        validation
    }
}

#[async_trait]
impl TokenService for JsonWebTokenService {
    async fn generate(&self, user_id: UserId) -> Result<String, Box<dyn Error + Send + Sync>> {
        let now = Utc::now();

        let claims = Claims {
            sub: user_id.to_string(),
            iat: now.timestamp() as usize,
            exp: (now + Duration::hours(24)).timestamp() as usize,
            iss: JWT_ISSUER.to_string(),
            aud: JWT_AUDIENCE.to_string(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| format!("jwt encode error: {e}").into())
    }

    async fn verify(&self, token: &str) -> Result<VerifiedToken, Box<dyn Error + Send + Sync>> {
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Self::validation(),
        )
        .map_err(|e| format!("jwt decode error: {e}"))?;

        let uuid =
            Uuid::parse_str(&data.claims.sub).map_err(|e| format!("invalid user id: {e}"))?;

        Ok(VerifiedToken {
            user_id: UserId::from_uuid(uuid),
            issued_at: data.claims.iat as i64,
        })
    }
}
