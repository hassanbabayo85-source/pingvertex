use axum::{
    body::Body,
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;

use domain::shared::types::{AuthenticatedUser, TenantContext};

use crate::auth_state::AuthState;

/// Middleware that authenticates a user from an access token, resolves
/// that user's single tenant membership, and inserts both the legacy
/// `UserId` and the new `AuthenticatedUser` into request extensions.
///
/// # Cookie-first token extraction (unchanged)
///
/// The existing cookie-sannan-header order is preserved: it prefers the
/// httpOnly `access_token` cookie and only falls back to the
/// `Authorization: Bearer ...` header when no cookie exists.
///
/// # Tenant resolution (fail closed)
///
/// The token alone proves `user_id`, not tenant membership. Since there is
/// no request-level tenant selector in this feature yet (no header/cookie
/// for "current tenant"), and since the backfill in
/// `0030_add_tenant_id_to_resources.sql` gives every user EXACTLY one
/// tenant (their own), this middleware resolves the user's tenant via
/// `list_by_user(user_id)`.
///
/// - If the user has exactly one membership, that membership is used.
/// - If the user has zero memberships, the request is rejected with
///   `UNAUTHORIZED`.
/// - If the user has more than one membership, the request is ALSO
///   rejected with `UNAUTHORIZED`. This case should not happen today
///   because there is no invite-flow yet, but this middleware fails
///   closed rather than guessing which tenant the caller meant. When a
///   real tenant selector is added later, this branch must be replaced
///   with an explicit selection made by the caller.
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

    let memberships = auth
        .tenant_membership_repository
        .list_by_user(user_id)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if memberships.len() != 1 {
        // Fail closed: either no tenant, or multiple tenants with no
        // request-level selector to choose between them.
        return Err(StatusCode::UNAUTHORIZED);
    }

    let membership = &memberships[0];
    let tenant_context = TenantContext::with_actor(membership.tenant_id, user_id);
    let authenticated_user = AuthenticatedUser::new(user_id, membership.role, tenant_context);

    // Keep inserting the plain `UserId` for backward compatibility with the
    // handlers that still depend on `Extension<UserId>`.
    request.extensions_mut().insert(user_id);
    request.extensions_mut().insert(authenticated_user);

    Ok(next.run(request).await)
}
