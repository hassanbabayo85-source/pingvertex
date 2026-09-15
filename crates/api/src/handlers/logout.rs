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
/// MATAKIN 4 — Logout is authentication-protected and does two things:
/// 1. expires the browser's ONLY access-token cookie (`__Host-access_token`); and
/// 2. writes a per-user "invalidated before" marker to Redis, revoking the
///    current JWT and every older JWT immediately.
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

    let primary = shared::cookie::logout_cookie();
    // We only use `insert` now since there is exactly one cookie.
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&primary)
            .map_err(|_| ApiError::internal("failed to clear cookie"))?,
    );

    Ok((headers, Json(json!({ "authenticated": false }))))
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn logout_clears_cookie_in_secure_mode() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let prev = std::env::var("ENVIRONMENT").ok();
        unsafe {
            std::env::set_var("ENVIRONMENT", "production");
        }

        let primary = shared::cookie::logout_cookie();

        unsafe {
            match prev {
                Some(v) => std::env::set_var("ENVIRONMENT", v),
                None => std::env::remove_var("ENVIRONMENT"),
            }
        }

        assert!(primary.starts_with("__Host-access_token=;"));
    }

    #[tokio::test]
    async fn logout_emits_single_cookie_in_dev_mode() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let prev = std::env::var("ENVIRONMENT").ok();
        unsafe {
            std::env::set_var("ENVIRONMENT", "development");
        }

        let primary = shared::cookie::logout_cookie();

        unsafe {
            match prev {
                Some(v) => std::env::set_var("ENVIRONMENT", v),
                None => std::env::remove_var("ENVIRONMENT"),
            }
        }

        assert!(primary.starts_with("access_token=;"));
    }

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
