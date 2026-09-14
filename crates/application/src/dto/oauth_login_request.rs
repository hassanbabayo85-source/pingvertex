use serde::{Deserialize, Serialize};

/// Body of `POST /auth/oauth/google` and `POST /auth/oauth/apple`.
///
/// `id_token` is the raw JWT credential the provider's own client-side
/// SDK hands back to the frontend after the user completes the
/// provider's sign-in flow — Google Identity Services' `credential`
/// field, or Apple JS's `authorization.id_token`. The backend never
/// sees a password or an OAuth client secret here; it only verifies
/// this token's signature against the provider's public keys (see
/// `application::ports::IdTokenVerifier`).
#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthLoginRequest {
    pub id_token: String,
}

/// PART 7 — manual Debug that redacts the OAuth id_token.
impl std::fmt::Debug for OAuthLoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthLoginRequest")
            .field("id_token", &"[REDACTED]")
            .finish()
    }
}
