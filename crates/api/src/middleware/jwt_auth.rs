use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;

use domain::shared::types::{AuthenticatedUser, TenantContext};
use domain::{TenantStatus, UserRole, UserStatus};
use uuid::Uuid;

use crate::auth_state::AuthState;

pub async fn jwt_auth(
    State(auth): State<AuthState>,
    jar: CookieJar,
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // MATAKIN 1 & 2 — ACCEPT ONLY __Host-access_token (or its dev equivalent).
    // The legacy "access_token" fallback iteration is removed. A valid
    // legacy cookie in the jar will be ignored entirely.
    let cookie_name = shared::cookie::access_token_cookie_name();
    let cookie_token = jar.get(&cookie_name).map(|c| c.value().to_string());

    let token = if let Some(token) = cookie_token {
        token
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

    // Reload the user's CURRENT account status.
    let user = auth
        .user_repository
        .find_by_id(&user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Email verification boundary.
    if matches!(
        user.status(),
        UserStatus::PendingVerification | UserStatus::Suspended | UserStatus::Deleted
    ) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let invalidated_at = auth
        .pending_auth_cache
        .get(&application::session_invalidated_at_cache_key(user_id))
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if let Some(invalidated_at_str) = invalidated_at {
        let invalidated_at = invalidated_at_str
            .parse::<i64>()
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        if verified.issued_at < invalidated_at {
            return Err(StatusCode::UNAUTHORIZED);
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

    let tenant = auth
        .tenant_repository
        .find_by_id(membership.tenant_id)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if tenant.status != TenantStatus::Active && user.role() != UserRole::Admin {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let tenant_context = TenantContext::with_actor(membership.tenant_id, user_id);
    let authenticated_user = AuthenticatedUser::new(user_id, membership.role, tenant_context)
        .with_platform_admin(matches!(user.role(), UserRole::Admin));

    request.extensions_mut().insert(user_id);
    request.extensions_mut().insert(authenticated_user);

    Ok(next.run(request).await)
}
