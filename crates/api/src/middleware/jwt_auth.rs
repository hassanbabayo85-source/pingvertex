use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;

use domain::shared::types::{AuthenticatedUser, TenantContext};
use uuid::Uuid;

use crate::auth_state::AuthState;

pub async fn jwt_auth(
    State(auth): State<AuthState>,
    jar: CookieJar,
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
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

    let verified = auth
        .token_service
        .verify(&token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let user_id = verified.user_id;

    if let Ok(Some(invalidated_at_str)) = auth
        .pending_auth_cache
        .get(&application::session_invalidated_at_cache_key(user_id))
        .await
    {
        if let Ok(invalidated_at) = invalidated_at_str.parse::<i64>() {
            if verified.issued_at < invalidated_at {
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    let memberships = auth
        .tenant_membership_repository
        .list_by_user(user_id)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if memberships.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let membership = if memberships.len() == 1 {
        &memberships[0]
    } else {
        let tenant_id = headers
            .get("x-tenant-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        memberships
            .iter()
            .find(|m| m.tenant_id.0 == tenant_id)
            .ok_or(StatusCode::UNAUTHORIZED)?
    };

    let tenant_context = TenantContext::with_actor(membership.tenant_id, user_id);
    let authenticated_user = AuthenticatedUser::new(user_id, membership.role, tenant_context);

    request.extensions_mut().insert(user_id);
    request.extensions_mut().insert(authenticated_user);

    Ok(next.run(request).await)
}
