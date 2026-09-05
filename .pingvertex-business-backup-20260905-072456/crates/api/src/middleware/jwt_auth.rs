use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;

use domain::UserStatus;
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

    // SECURITY (P0 — fixed): reload the user's CURRENT account status on
    // every authenticated request. Without this, a JWT issued before an
    // account was suspended or soft-deleted stayed valid until it
    // naturally expired (up to 24h), regardless of any subsequent
    // administrator action or incident-response decision. Suspension
    // and deletion are authorization events and must take effect
    // immediately — not "the next time the user happens to log in
    // again". This check runs BEFORE `AuthenticatedUser` is
    // constructed, so a non-active account never even reaches a
    // handler.
    let user = auth
        .user_repository
        .find_by_id(&user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // SECURITY: email verification is a hard authentication boundary.
    // A token must never grant dashboard/API access to an unverified account.
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

    let tenant_context = TenantContext::with_actor(membership.tenant_id, user_id);
    let authenticated_user = AuthenticatedUser::new(user_id, membership.role, tenant_context);

    request.extensions_mut().insert(user_id);
    request.extensions_mut().insert(authenticated_user);

    Ok(next.run(request).await)
}
