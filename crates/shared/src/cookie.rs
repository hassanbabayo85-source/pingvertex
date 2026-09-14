//! Helpers for building the `Set-Cookie` header used across every
//! authentication success path (login, register, OAuth, TOTP verify).
//!
//! Centralized here so all call sites stay in sync, and so the
//! `Secure` attribute can be environment-aware without touching
//! `AuthState`/`Config` at every call site: it mirrors the same
//! allowlist approach used by `shared::ssrf` and
//! `api::config::noop_implementations_allowed`, and fails SAFE — any
//! unset/unrecognized `ENVIRONMENT` value defaults to `Secure`.
//!
//! PART 6 — cookie hardening
//! -------------------------
//! In secure mode the cookie is named `__Host-access_token`, which the
//! browser enforces as: must be `Secure`, must be from a secure origin
//! (HTTPS), must have `Path=/`, and must NOT carry a `Domain` attribute.
//! Those constraints make it impossible for a subdomain (or an
//! attacker-controlled sibling subdomain) to overwrite the session
//! cookie — a class of attack called "cookie tossing".
//!
//! In dev/test mode over HTTP, the browser would reject a `__Host-`
//! cookie outright, so we fall back to the legacy `access_token` name.
//! The middleware reads BOTH names during the transition, and logout
//! clears BOTH.

/// Environments in which `Secure` is disabled and the legacy cookie name
/// is used. Mirrors `api::config::NOOP_ALLOWED_ENVIRONMENTS` (kept here
/// too so `shared` does not have to depend on `api`).
const SAFE_NON_PRODUCTION_ENVIRONMENTS: &[&str] = &["development", "dev", "test", "local"];

/// `true` unless `ENVIRONMENT` is explicitly one of the recognized
/// local/dev/test values. Never relaxed by omission — only an explicit,
/// recognized env var value turns `Secure` off.
pub fn secure_cookies_enabled() -> bool {
    let environment = std::env::var("ENVIRONMENT").unwrap_or_default();
    !SAFE_NON_PRODUCTION_ENVIRONMENTS
        .iter()
        .any(|safe| safe.eq_ignore_ascii_case(environment.trim()))
}

/// The cookie name to use for the CURRENT environment.
///
/// In secure mode this is the `__Host-` prefixed name; in dev it is the
/// legacy `access_token` name. Consumers MUST read both via
/// [`access_token_cookie_names`] during the transition; the writer only
/// emits the environment-appropriate name.
pub const SECURE_COOKIE_NAME: &str = "__Host-access_token";
pub const LEGACY_COOKIE_NAME: &str = "access_token";

pub fn access_token_cookie_name() -> &'static str {
    if secure_cookies_enabled() {
        SECURE_COOKIE_NAME
    } else {
        LEGACY_COOKIE_NAME
    }
}

/// The names a reader should try, in order. During the transition a
/// production deployment may still be receiving the legacy cookie from a
/// client that hasn't re-logged-in yet; both are accepted by `jwt_auth`.
pub fn access_token_cookie_names() -> &'static [&'static str] {
    if secure_cookies_enabled() {
        &[SECURE_COOKIE_NAME, LEGACY_COOKIE_NAME]
    } else {
        &[LEGACY_COOKIE_NAME]
    }
}

/// Builds the `Set-Cookie` value for a freshly issued session JWT.
/// Identical across login/register/OAuth/TOTP-verify success paths.
pub fn access_token_cookie(token: &str) -> String {
    let name = access_token_cookie_name();
    if secure_cookies_enabled() {
        // No `Domain` attribute is intentionally omitted — `__Host-`
        // requires its absence, and browsers reject the cookie if
        // present. `Path=/` is likewise required by `__Host-`.
        format!("{name}={token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=86400")
    } else {
        format!("{name}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400")
    }
}

/// Builds the `Set-Cookie` value that clears the session cookie on logout.
/// Attributes must match `access_token_cookie`'s (aside from the empty
/// value and `Max-Age=0`) for browsers to reliably delete it.
pub fn logout_cookie() -> String {
    let name = access_token_cookie_name();
    if secure_cookies_enabled() {
        format!("{name}=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0")
    } else {
        format!("{name}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
    }
}

/// Additional Set-Cookie value that clears the LEGACY cookie name during
/// the transition to `__Host-access_token`. Only emitted in secure mode
/// (in dev the legacy name IS the current name, so a single
/// `logout_cookie()` is enough).
pub fn legacy_logout_cookie() -> Option<String> {
    if secure_cookies_enabled() {
        Some(format!(
            "{LEGACY_COOKIE_NAME}=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0"
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_test_lock::ENV_MUTEX as ENV_LOCK;

    #[test]
    fn defaults_to_secure_when_environment_unset() {
        let _guard = ENV_LOCK.blocking_lock();
        unsafe {
            std::env::remove_var("ENVIRONMENT");
        }
        assert!(secure_cookies_enabled());
        assert!(access_token_cookie("tok").contains("Secure"));
        // PART 6 — the secure cookie name is the __Host- prefixed one.
        assert!(access_token_cookie("tok").starts_with("__Host-access_token="));
    }

    #[test]
    fn disables_secure_for_recognized_dev_environments() {
        let _guard = ENV_LOCK.blocking_lock();
        for value in ["development", "dev", "test", "local", "DEVELOPMENT"] {
            unsafe {
                std::env::set_var("ENVIRONMENT", value);
            }
            assert!(
                !secure_cookies_enabled(),
                "expected '{value}' to disable Secure"
            );
            assert!(!access_token_cookie("tok").contains("Secure"));
            // In dev the __Host- prefix would be rejected by the browser
            // (no HTTPS), so the legacy name is used.
            assert!(access_token_cookie("tok").starts_with("access_token="));
        }
        unsafe {
            std::env::remove_var("ENVIRONMENT");
        }
    }

    #[test]
    fn stays_secure_for_unrecognized_environment_value() {
        let _guard = ENV_LOCK.blocking_lock();
        for value in ["staging", "preprod", "uat", "banana"] {
            unsafe {
                std::env::set_var("ENVIRONMENT", value);
            }
            assert!(
                secure_cookies_enabled(),
                "unrecognized env '{value}' must default to Secure"
            );
            assert!(access_token_cookie("tok").contains("Secure"));
            assert!(access_token_cookie("tok").starts_with("__Host-access_token="));
        }
        unsafe {
            std::env::remove_var("ENVIRONMENT");
        }
    }

    #[test]
    fn secure_cookie_has_all_required_attributes() {
        let _guard = ENV_LOCK.blocking_lock();
        unsafe {
            std::env::set_var("ENVIRONMENT", "production");
        }
        let cookie = access_token_cookie("tok");
        assert!(cookie.contains("Secure"), "missing Secure");
        assert!(cookie.contains("HttpOnly"), "missing HttpOnly");
        assert!(
            cookie.contains("SameSite=Strict"),
            "missing SameSite=Strict"
        );
        assert!(cookie.contains("Path=/"), "missing Path=/");
        assert!(
            !cookie.contains("Domain"),
            "__Host- cookie must NOT carry a Domain attribute"
        );
        unsafe {
            std::env::remove_var("ENVIRONMENT");
        }
    }

    #[test]
    fn logout_clears_both_names_in_secure_mode() {
        let _guard = ENV_LOCK.blocking_lock();
        unsafe {
            std::env::set_var("ENVIRONMENT", "production");
        }
        assert!(logout_cookie().starts_with("__Host-access_token=;"));
        let legacy = legacy_logout_cookie().expect("legacy clear must be emitted in secure mode");
        assert!(legacy.starts_with("access_token=;"));
        assert!(legacy.contains("Max-Age=0"));
        unsafe {
            std::env::remove_var("ENVIRONMENT");
        }
    }

    #[test]
    fn logout_does_not_emit_legacy_clear_in_dev() {
        let _guard = ENV_LOCK.blocking_lock();
        unsafe {
            std::env::set_var("ENVIRONMENT", "development");
        }
        assert!(legacy_logout_cookie().is_none());
        unsafe {
            std::env::remove_var("ENVIRONMENT");
        }
    }
}
