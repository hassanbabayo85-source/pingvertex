use axum::Json;
use axum::http::header::{HeaderMap, HeaderValue, SET_COOKIE};
use serde_json::{Value, json};

use crate::error::ApiError;

/// `POST /api/v1/auth/logout`
///
/// Clears the `access_token` httpOnly cookie set by `login`/`register` by
/// re-issuing it with `Max-Age=0` (and no value) — the standard way to
/// delete a cookie, since JavaScript can't touch an httpOnly cookie
/// directly. The frontend's previous "logout" only cleared a local
/// `localStorage` UI flag and left the cookie itself (and the JWT inside
/// it) fully valid until its 24h expiry, so on a shared machine a
/// "logged out" session could still authenticate every API request.
///
/// Attributes (`HttpOnly`, `Secure`, `SameSite=Strict`, `Path=/`) must
/// match the ones `login`/`register` used when setting the cookie —
/// browsers only overwrite/delete a cookie when path (and, if set,
/// domain) match exactly.
///
/// This does not maintain a server-side token denylist, so a JWT that was
/// copied out of the cookie before logout (e.g. via a prior XSS, or a
/// proxy/log capture) remains valid until it expires on its own. Closing
/// that gap needs a `jti`-based denylist or a move to short-lived access
/// tokens + refresh tokens, which is a bigger change than a logout route
/// alone — tracked as a follow-up, not something this endpoint alone can
/// promise.
pub async fn logout() -> Result<(HeaderMap, Json<Value>), ApiError> {
    let cookie = "access_token=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0";

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(cookie).map_err(|_| ApiError::internal("failed to clear cookie"))?,
    );

    Ok((headers, Json(json!({ "authenticated": false }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn clears_the_access_token_cookie() {
        let (headers, body) = logout().await.expect("logout should not fail");

        let set_cookie = headers
            .get(SET_COOKIE)
            .expect("logout must set a Set-Cookie header")
            .to_str()
            .unwrap();

        assert!(set_cookie.starts_with("access_token=;"));
        assert!(set_cookie.contains("Max-Age=0"));
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("Secure"));
        assert!(set_cookie.contains("SameSite=Strict"));
        assert!(set_cookie.contains("Path=/"));
        assert_eq!(body.0["authenticated"], false);
    }
}
