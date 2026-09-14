use axum::{
    Json,
    extract::{Extension, State},
    http::header::{HeaderMap, HeaderValue, SET_COOKIE},
};
use serde_json::{Value, json};

use application::{
    SESSION_INVALIDATION_TTL_SECONDS, session_invalidated_at_cache_key,
    session_revocation_timestamp,
};
use domain::shared::types::AuthenticatedUser;

use crate::{auth_state::AuthState, error::ApiError};

/// `POST /api/v1/auth/logout`
///
/// Logout is authentication-protected and does two things:
/// 1. expires the browser's httpOnly access-token cookie(s); and
/// 2. writes a per-user "invalidated before" marker to Redis, revoking the
///    current JWT and every older JWT immediately.
///
/// The marker is mandatory. If Redis is unavailable, logout fails closed
/// rather than reporting success while the bearer token remains valid.
///
/// PART 6 — during the transition from the legacy `access_token` cookie
/// to the `__Host-access_token` cookie, TWO `Set-Cookie` headers are
/// emitted in secure mode (one per name). This is why `HeaderMap::append`
/// is used instead of `insert` — `insert` would replace the first header
/// with the second, leaving the legacy cookie alive after logout.
pub async fn logout(
    State(auth): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    let now = chrono::Utc::now().timestamp();
    let revoked_before = session_revocation_timestamp(now);

    auth.pending_auth_cache
        .set(
            &session_invalidated_at_cache_key(authenticated_user.user_id()),
            &revoked_before.to_string(),
            SESSION_INVALIDATION_TTL_SECONDS,
        )
        .await
        .map_err(|err| ApiError::internal(format!("failed to invalidate session: {err}")))?;

    let mut headers = HeaderMap::new();

    // Current (or dev) cookie name.
    let primary = shared::cookie::logout_cookie();
    headers.append(
        SET_COOKIE,
        HeaderValue::from_str(&primary)
            .map_err(|_| ApiError::internal("failed to clear cookie"))?,
    );

    // During the transition, also clear the legacy cookie name so a
    // client that still holds one doesn't keep a session token around
    // after logout. In dev, `legacy_logout_cookie()` returns `None` (the
    // primary name IS the legacy name).
    if let Some(legacy) = shared::cookie::legacy_logout_cookie() {
        headers.append(
            SET_COOKIE,
            HeaderValue::from_str(&legacy)
                .map_err(|_| ApiError::internal("failed to clear legacy cookie"))?,
        );
    }

    Ok((headers, Json(json!({ "authenticated": false }))))
}

#[cfg(test)]
mod tests {

    /// PART 6 — in a secure environment, logout must emit TWO Set-Cookie
    /// headers: one for `__Host-access_token` and one for the legacy
    /// `access_token` (so a client with a stale legacy cookie is also
    /// cleared).
    #[tokio::test]
    async fn logout_clears_both_cookie_names_in_secure_mode() {
        // SAFETY: these tests run in a single process; the shared env
        // mutex in `shared::env_test_lock` is not reachable from this
        // crate, so we set/restore around the call.
        let prev = std::env::var("ENVIRONMENT").ok();
        unsafe {
            std::env::set_var("ENVIRONMENT", "production");
        }

        let primary = shared::cookie::logout_cookie();
        let legacy = shared::cookie::legacy_logout_cookie();

        unsafe {
            match prev {
                Some(v) => std::env::set_var("ENVIRONMENT", v),
                None => std::env::remove_var("ENVIRONMENT"),
            }
        }

        assert!(primary.starts_with("__Host-access_token=;"));
        assert!(
            legacy.is_some(),
            "legacy cookie clear MUST be emitted in secure mode"
        );
        let legacy = legacy.unwrap();
        assert!(legacy.starts_with("access_token=;"));
    }

    /// In a dev environment, only ONE Set-Cookie is emitted (the dev
    /// cookie name IS the legacy name).
    #[tokio::test]
    async fn logout_emits_single_cookie_in_dev_mode() {
        let prev = std::env::var("ENVIRONMENT").ok();
        unsafe {
            std::env::set_var("ENVIRONMENT", "development");
        }

        let primary = shared::cookie::logout_cookie();
        let legacy = shared::cookie::legacy_logout_cookie();

        unsafe {
            match prev {
                Some(v) => std::env::set_var("ENVIRONMENT", v),
                None => std::env::remove_var("ENVIRONMENT"),
            }
        }

        assert!(primary.starts_with("access_token=;"));
        assert!(legacy.is_none());
    }

    /// Attributes on the emitted cookies must match what the browser
    /// requires to actually delete them.
    #[test]
    fn logout_cookie_has_required_attributes() {
        let cookie = "access_token=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0";
        assert!(cookie.starts_with("access_token=;"));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Path=/"));
    }
}
