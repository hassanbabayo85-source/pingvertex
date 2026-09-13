use application::ports::TokenService;
use chrono::{Duration, Utc};
use infra::JsonWebTokenService;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::Serialize;

const SECRET: &str = "jwt-claims-regression-secret-must-be-long-enough";

#[derive(Serialize)]
struct FlexibleClaims {
    sub: String,
    iat: usize,
    exp: usize,
    iss: String,
    aud: String,
}

fn encoded(user_id: &str, iss: &str, aud: &str) -> String {
    let now = Utc::now();
    let claims = FlexibleClaims {
        sub: user_id.to_string(),
        iat: now.timestamp() as usize,
        exp: (now + Duration::hours(1)).timestamp() as usize,
        iss: iss.to_string(),
        aud: aud.to_string(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

#[tokio::test]
async fn valid_token_round_trips() {
    let service = JsonWebTokenService::new(SECRET);
    let user_id = domain::UserId::new();

    let token = service.generate(user_id).await.unwrap();
    let verified = service.verify(&token).await.unwrap();
    assert_eq!(verified.user_id, user_id);
}

#[tokio::test]
async fn wrong_issuer_is_rejected() {
    let service = JsonWebTokenService::new(SECRET);

    let token = encoded(
        &domain::UserId::new().to_string(),
        "https://evil.example.com",
        "uptime-monitor-api",
    );

    let err = service.verify(&token).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("InvalidIssuer") || msg.contains("issuer") || msg.contains("jwt decode error"),
        "wrong issuer must be rejected, got: {msg}"
    );
}

#[tokio::test]
async fn wrong_audience_is_rejected() {
    let service = JsonWebTokenService::new(SECRET);

    let token = encoded(
        &domain::UserId::new().to_string(),
        "uptime-monitor",
        "some-other-api",
    );

    let err = service.verify(&token).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("InvalidAudience")
            || msg.contains("audience")
            || msg.contains("jwt decode error"),
        "wrong audience must be rejected, got: {msg}"
    );
}

#[tokio::test]
async fn token_missing_iss_and_aud_is_rejected() {
    #[derive(Serialize)]
    struct LegacyClaims {
        sub: String,
        iat: usize,
        exp: usize,
    }

    let now = Utc::now();
    let claims = LegacyClaims {
        sub: domain::UserId::new().to_string(),
        iat: now.timestamp() as usize,
        exp: (now + Duration::hours(1)).timestamp() as usize,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap();

    let service = JsonWebTokenService::new(SECRET);
    assert!(service.verify(&token).await.is_err());
}

#[tokio::test]
async fn tampered_signature_is_rejected() {
    let service = JsonWebTokenService::new(SECRET);
    let mut token = service.generate(domain::UserId::new()).await.unwrap();

    let last = token.pop().unwrap();
    let replacement = if last == 'A' { 'B' } else { 'A' };
    token.push(replacement);

    assert!(service.verify(&token).await.is_err());
}
