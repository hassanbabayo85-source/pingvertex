//! Helpers for building the `Set-Cookie` header used across every
//! authentication success path (login, register, OAuth, TOTP verify).
//!
//! Centralized here so all six call sites stay in sync, and so the
//! `Secure` attribute can be environment-aware without touching
//! `AuthState`/`Config` at every call site: it mirrors the same
//! `SAFE_NON_PRODUCTION_ENVIRONMENTS` allow-list already used by
//! `shared::ssrf::validate_production_ssrf_config`, and fails SAFE —
//! any unset/unrecognized `ENVIRONMENT` value defaults to `Secure`.

const SAFE_NON_PRODUCTION_ENVIRONMENTS: &[&str] = &["development", "dev", "test", "local"];

/// `true` unless `ENVIRONMENT` is explicitly one of the recognized
/// local/dev/test values. Never relaxed by omission — only an explicit,
/// recognized env var value turns `Secure` off, so a misconfigured or
/// empty `ENVIRONMENT` in production still gets a `Secure` cookie.
pub fn secure_cookies_enabled() -> bool {
    let environment = std::env::var("ENVIRONMENT").unwrap_or_default();
    !SAFE_NON_PRODUCTION_ENVIRONMENTS
        .iter()
        .any(|safe| safe.eq_ignore_ascii_case(environment.trim()))
}

/// Builds the `Set-Cookie` value for a freshly issued session JWT.
/// Identical across login/register/OAuth/TOTP-verify success paths.
pub fn access_token_cookie(token: &str) -> String {
    if secure_cookies_enabled() {
        format!("access_token={token}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=86400")
    } else {
        format!("access_token={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400")
    }
}

/// Builds the `Set-Cookie` value that clears the session cookie on logout.
/// Attributes must match `access_token_cookie`'s (aside from the empty
/// value and `Max-Age=0`) for browsers to reliably delete it.
pub fn logout_cookie() -> String {
    if secure_cookies_enabled() {
        "access_token=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0".to_string()
    } else {
        "access_token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_test_lock::ENV_MUTEX as ENV_LOCK;

    #[test]
    fn defaults_to_secure_when_environment_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("ENVIRONMENT");
        }
        assert!(secure_cookies_enabled());
        assert!(access_token_cookie("tok").contains("Secure"));
    }

    #[test]
    fn disables_secure_for_recognized_dev_environments() {
        let _guard = ENV_LOCK.lock().unwrap();
        for value in ["development", "dev", "test", "local", "DEVELOPMENT"] {
            unsafe {
                std::env::set_var("ENVIRONMENT", value);
            }
            assert!(
                !secure_cookies_enabled(),
                "expected '{value}' to disable Secure"
            );
            assert!(!access_token_cookie("tok").contains("Secure"));
        }
        unsafe {
            std::env::remove_var("ENVIRONMENT");
        }
    }

    #[test]
    fn stays_secure_for_unrecognized_environment_value() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("ENVIRONMENT", "staging");
        }
        assert!(secure_cookies_enabled());
        unsafe {
            std::env::remove_var("ENVIRONMENT");
        }
    }
}
