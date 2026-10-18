//! Helpers for creating the standard session authentication cookie.
//!
//! # The `__Host-` Prefix Hardening
//!
//! To prevent "cookie tossing" attacks where a sub-domain (e.g.,
//! `attacker.app.pingvertex.com` or `dev-site.app.pingvertex.com`)
//! could inject a fraudulent `access_token` cookie into requests bound for
//! the primary domain, production authentication cookies use the
//! `__Host-` prefix (`__Host-access_token`).
//!
//! The `__Host-` prefix enforces that the browser will ONLY accept the
//! `Set-Cookie` header if:
//! 1. The connection is HTTPS (`Secure` attribute is present).
//! 2. The cookie is NOT scoped to a domain (`Domain` attribute is absent).
//! 3. The path is explicitly set to `/` (`Path=/`).
//!
//! By forbidding the `Domain` attribute, the cookie is "locked" strictly
//! to the exact host that issued it. It will not be sent to any subdomains,
//! and no subdomain can ever overwrite it for the parent domain.
//!
//! This rule is followed strictly and there is no *legacy cookie* fallback.

use std::env;

/// The single standard name for the session cookie.
pub fn access_token_cookie_name() -> String {
    // Mun cire fallback, ONLY __Host-access_token is valid in all secure environments.
    // However, the `__Host-` prefix literally requires HTTPS. For local
    // development (e.g. `http://localhost:3000`), the browser will silently
    // reject a `__Host-` cookie. Thus, we keep a non-prefixed name for local dev only.
    if is_secure_environment() {
        "__Host-access_token".to_string()
    } else {
        "access_token".to_string()
    }
}

/// Returns true if the environment requires secure cookies (HTTPS).
/// Any environment other than local/test/dev is considered secure.
pub fn is_secure_environment() -> bool {
    let env = env::var("ENVIRONMENT").unwrap_or_default();
    let normalized = env.trim().to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "local" | "test" | "dev" | "development"
    )
}

/// Builds the `Set-Cookie` header string for a new session.
/// Enforces the required cookie attributes.
pub fn access_token_cookie(token: &str) -> String {
    let name = access_token_cookie_name();
    if is_secure_environment() {
        // Production: __Host- prefix MUST have Secure, Path=/, and NO Domain.
        format!("{name}={token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=86400")
    } else {
        // Local development over HTTP. We use SameSite=Lax so OAuth works correctly.
        format!("{name}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400")
    }
}

/// Builds the `Set-Cookie` header string to clear the session cookie.
pub fn logout_cookie() -> String {
    let name = access_token_cookie_name();
    if is_secure_environment() {
        format!("{name}=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0")
    } else {
        format!("{name}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
    }
}
