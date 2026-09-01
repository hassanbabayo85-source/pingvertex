use std::error::Error;
use std::time::{Duration, Instant};

use application::ports::{IdTokenVerifier, OidcVerifierResult, VerifiedIdentity};
use async_trait::async_trait;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

/// How long a fetched JWKS is trusted before being re-fetched on the
/// next verification. Google/Apple rotate signing keys infrequently
/// (weeks), so this is generous — it just needs to be short enough
/// that a rotation propagates without a redeploy.
const JWKS_CACHE_TTL: Duration = Duration::from_secs(600);

#[derive(Debug, Deserialize)]
struct IdTokenClaims {
    sub: String,
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<serde_json::Value>,
}

struct JwksCache {
    fetched_at: Instant,
    jwks: JwkSet,
}

/// Verifies RS256 `id_token`s issued by a single OpenID-Connect
/// identity provider (Google or Apple), against that provider's public
/// JWKS — fetched over HTTPS and short-term cached, never
/// hard-coded/vendored, so key rotation on the provider's side needs no
/// redeploy here.
///
/// One instance per provider: construct via [`Self::google`] or
/// [`Self::apple`]. Both providers speak the same JWT/JWKS shape, so a
/// single generic implementation covers both — only the JWKS URL,
/// accepted issuer(s), and expected audience (client ID) differ.
pub struct OidcIdTokenVerifier {
    jwks_url: String,
    expected_issuers: Vec<String>,
    client_id: String,
    http_client: reqwest::Client,
    cache: RwLock<Option<JwksCache>>,
}

impl OidcIdTokenVerifier {
    pub fn new(
        jwks_url: impl Into<String>,
        expected_issuers: Vec<String>,
        client_id: impl Into<String>,
    ) -> Self {
        Self {
            jwks_url: jwks_url.into(),
            expected_issuers,
            client_id: client_id.into(),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("valid reqwest client configuration"),
            cache: RwLock::new(None),
        }
    }

    /// `client_id` is your Google OAuth 2.0 Web Client ID (the same
    /// value the frontend passes to Google Identity Services as
    /// `client_id`) — it must match the `aud` claim on every token this
    /// verifier accepts.
    pub fn google(client_id: impl Into<String>) -> Self {
        Self::new(
            "https://www.googleapis.com/oauth2/v3/certs",
            vec![
                "https://accounts.google.com".to_string(),
                "accounts.google.com".to_string(),
            ],
            client_id,
        )
    }

    /// `client_id` is your Apple Services ID identifier (e.g.
    /// `com.yourcompany.pingvertex.web`) — the same value the frontend
    /// passes to Apple JS as `clientId`.
    pub fn apple(client_id: impl Into<String>) -> Self {
        Self::new(
            "https://appleid.apple.com/auth/keys",
            vec!["https://appleid.apple.com".to_string()],
            client_id,
        )
    }

    async fn fetch_jwks(&self) -> Result<JwkSet, Box<dyn Error + Send + Sync>> {
        let jwks: JwkSet = self
            .http_client
            .get(&self.jwks_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(jwks)
    }

    async fn jwks(&self, force_refresh: bool) -> Result<JwkSet, Box<dyn Error + Send + Sync>> {
        if !force_refresh {
            let cached = self.cache.read().await;
            if let Some(entry) = cached.as_ref() {
                if entry.fetched_at.elapsed() < JWKS_CACHE_TTL {
                    return Ok(entry.jwks.clone());
                }
            }
        }

        let jwks = self.fetch_jwks().await?;
        let mut cached = self.cache.write().await;
        *cached = Some(JwksCache {
            fetched_at: Instant::now(),
            jwks: jwks.clone(),
        });
        Ok(jwks)
    }
}

#[async_trait]
impl IdTokenVerifier for OidcIdTokenVerifier {
    async fn verify(&self, id_token: &str) -> OidcVerifierResult<VerifiedIdentity> {
        let header = decode_header(id_token)?;
        let kid = header.kid.ok_or("id_token is missing a 'kid' header")?;

        // Try the cached key set first. If the key isn't found — the
        // provider may have rotated keys since our last fetch — force
        // exactly one refresh before giving up, so rotation on the
        // provider's side doesn't require a redeploy here.
        let mut jwks = self.jwks(false).await?;
        let mut jwk = jwks.find(&kid).cloned();
        if jwk.is_none() {
            jwks = self.jwks(true).await?;
            jwk = jwks.find(&kid).cloned();
        }
        let jwk = jwk.ok_or("no matching signing key found for this id_token")?;

        let decoding_key = DecodingKey::from_jwk(&jwk)?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[self.client_id.clone()]);
        validation.set_issuer(&self.expected_issuers);

        let data = decode::<IdTokenClaims>(id_token, &decoding_key, &validation)?;
        let claims = data.claims;

        let email = claims
            .email
            .ok_or("id_token did not include an email claim")?;

        // Google encodes `email_verified` as a JSON bool; Apple
        // sometimes encodes it as the string `"true"`/`"false"`
        // instead — normalize both rather than trusting serde to pick
        // one shape.
        let email_verified = match claims.email_verified {
            Some(serde_json::Value::Bool(value)) => value,
            Some(serde_json::Value::String(value)) => value == "true",
            _ => false,
        };

        Ok(VerifiedIdentity {
            email,
            email_verified,
            subject: claims.sub,
        })
    }
}

/// Fail-loud fallback used when a provider's client ID is not
/// configured in this deployment's environment. Mirrors
/// `DisabledEmailVerificationMailer`/`DisabledPasswordResetMailer`:
/// never silently no-ops, always returns a clear, provider-named error
/// so a misconfiguration shows up immediately in logs/response rather
/// than as a confusing generic failure.
pub struct DisabledIdTokenVerifier {
    provider: String,
}

impl DisabledIdTokenVerifier {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
        }
    }
}

#[async_trait]
impl IdTokenVerifier for DisabledIdTokenVerifier {
    async fn verify(&self, _id_token: &str) -> OidcVerifierResult<VerifiedIdentity> {
        Err(format!("{} sign-in is not configured on this server", self.provider).into())
    }
}
