use std::error::Error;

use async_trait::async_trait;

pub type OidcVerifierResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Identity claims extracted from a verified OIDC `id_token` (Google
/// "Sign in with Google" / Apple "Sign in with Apple"), once its
/// signature, issuer, audience, and expiry have all been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    /// The email address the identity provider vouches for. Always
    /// present — `OAuthLoginUseCase` cannot provision or match an
    /// account without one.
    pub email: String,
    /// Whether the provider itself considers this email verified.
    /// Google/Apple only ever hand out an `id_token` for an email they
    /// have already verified, so this is true in practice; kept
    /// separate from `email` so callers can log/audit the (rare) case
    /// where a provider reports `false`, rather than silently trusting
    /// it.
    pub email_verified: bool,
    /// The provider's stable, opaque subject identifier (`sub` claim)
    /// for this user. Not currently persisted — account matching is by
    /// verified email (see `OAuthLoginUseCase`) — but kept on the
    /// struct so a future dedicated `oauth_identities` table can link
    /// on it without changing this port's signature.
    pub subject: String,
}

/// Verifies a raw OIDC `id_token` (JWT) against a specific identity
/// provider's public JWKS, issuer, and expected audience (client ID),
/// and returns the identity it attests to.
///
/// One implementation per provider (`OidcIdTokenVerifier` in `infra` is
/// generic and configured per-provider at construction time); the
/// application layer never knows Google/Apple-specific detail beyond
/// which provider's verifier it was handed.
#[async_trait]
pub trait IdTokenVerifier: Send + Sync {
    async fn verify(&self, id_token: &str) -> OidcVerifierResult<VerifiedIdentity>;
}

use std::sync::Arc;

#[async_trait]
impl<T> IdTokenVerifier for Arc<T>
where
    T: IdTokenVerifier + ?Sized,
{
    async fn verify(&self, id_token: &str) -> OidcVerifierResult<VerifiedIdentity> {
        (**self).verify(id_token).await
    }
}
