use axum::{
    body::Body,
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;

use crate::auth_state::AuthState;

pub async fn jwt_auth(
    State(auth): State<AuthState>,
    jar: CookieJar,
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Fara duba httpOnly cookie (sabon tsari, mafi aminci), sannan
    // Authorization header a matsayin madadin (backwards-compatible).
    let token = if let Some(cookie) = jar.get("access_token") {
        cookie.value().to_string()
    } else {
        let authorization = headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        authorization
            .strip_prefix("Bearer ")
            .ok_or(StatusCode::UNAUTHORIZED)?
            .to_string()
    };

    let user_id = auth
        .token_service
        .verify(&token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    request.extensions_mut().insert(user_id);

    Ok(next.run(request).await)
}
