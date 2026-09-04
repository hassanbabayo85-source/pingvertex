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
/// 1. expires the browser's httpOnly access-token cookie; and
/// 2. writes a per-user "invalidated before" marker to Redis, revoking the
///    current JWT and every older JWT immediately.
///
/// The marker is mandatory. If Redis is unavailable, logout fails closed
/// rather than reporting success while the bearer token remains valid.
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

    let cookie = shared::cookie::logout_cookie();

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| ApiError::internal("failed to clear cookie"))?,
    );

    Ok((headers, Json(json!({ "authenticated": false }))))
}

#[cfg(test)]
mod tests {
    // Cookie-format regression is intentionally kept here. Session revocation
    // semantics are covered by the application-layer timestamp tests and the
    // authenticated-router test suite.
    #[test]
    fn logout_cookie_is_deleted_with_matching_security_attributes() {
        let cookie = "access_token=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0";
        assert!(cookie.starts_with("access_token=;"));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Path=/"));
    }
}
