use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use chrono::Utc;

use domain::{
    enums::ApiKeyScope,
    shared::ids::{ApiKeyId, TenantId, UserId},
};

use crate::state::AppState;

/// Request-scoped identity established by `ApiKeyAuthMiddleware`, carrying
/// exactly the information downstream handlers need: which tenant/user the
/// key belongs to, and which scopes it was granted. Deliberately separate
/// from `domain::shared::types::AuthenticatedUser` (used by `jwt_auth`) —
/// an API key has explicit scopes, not a coarse-grained `Role`, so it is
/// not safe to synthesize an `AuthenticatedUser` from one (that would
/// require inventing a `Role` whose permission set doesn't actually match
/// what the key was scoped to).
#[derive(Debug, Clone, Copy)]
pub struct ApiKeyPrincipal {
    pub api_key_id: ApiKeyId,
    pub tenant_id: TenantId,
    pub user_id: UserId,
}

/// Scopes granted to the key that authenticated the current request.
/// Kept as a separate extension (rather than a field on `ApiKeyPrincipal`)
/// only so `require_api_key_scope` can extract just the scopes without
/// cloning the rest — in practice both are always inserted together.
#[derive(Debug, Clone)]
pub struct ApiKeyScopes(pub Vec<ApiKeyScope>);

/// Middleware that authenticates a request using the `X-API-Key` header
/// instead of a session/JWT.
///
/// # Lookup, not a table scan
///
/// Argon2 hashes are salted, so a raw key can't be found by an equality
/// lookup against `hashed_key`. Every raw key is therefore formatted as
/// `pv_<prefix>_<secret>` (see `IdGenerator::next_api_key_secret`); this
/// middleware splits off the `pv_<prefix>` portion and looks the row up by
/// that plaintext, indexed, unique column
/// (`ApiKeyRepository::find_by_prefix`) — O(1) instead of O(n) Argon2
/// verifications.
///
/// # Fail closed
///
/// Any of the following results in `401 UNAUTHORIZED`: missing header,
/// malformed key (no `pv_<prefix>_<secret>` shape), unknown prefix, hash
/// mismatch, or an expired key. There is no partial-success state.
///
/// # Scope enforcement
///
/// This middleware only AUTHENTICATES the key and inserts its scopes into
/// request extensions as `ApiKeyScopes`. Per-route scope REQUIREMENTS are
/// enforced separately by `require_api_key_scope`, layered after this
/// middleware — the same two-step pattern `jwt_auth` +
/// `require_permission` already uses for session-authenticated routes.
pub async fn api_key_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let raw_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // "pv_<prefix>_<secret>" — the prefix is everything before the LAST
    // underscore, matching how `CreateApiKeyUseCase` derives it.
    let (key_prefix, _secret) = raw_key.rsplit_once('_').ok_or(StatusCode::UNAUTHORIZED)?;

    let api_key = state
        .api_key_repository
        .find_by_prefix(key_prefix)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if api_key.is_expired(Utc::now()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let verified = state
        .password_hasher
        .verify(raw_key, &api_key.hashed_key)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if !verified {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Best-effort bookkeeping — a failure here must never block the
    // request the key legitimately authenticated.
    let _ = state
        .api_key_repository
        .touch_last_used(api_key.id, Utc::now())
        .await;

    request.extensions_mut().insert(ApiKeyPrincipal {
        api_key_id: api_key.id,
        tenant_id: api_key.tenant_id,
        user_id: api_key.user_id,
    });
    request
        .extensions_mut()
        .insert(ApiKeyScopes(api_key.scopes));

    Ok(next.run(request).await)
}

/// Axum middleware that requires the request to already contain
/// `ApiKeyScopes` in extensions (inserted by `api_key_auth`, which MUST be
/// layered before this) and for that key to have been granted `scope`.
///
/// Mirrors `require_permission`: if no `ApiKeyScopes` is present, this is
/// `UNAUTHORIZED`; if the key is authenticated but lacks the scope, this
/// is `FORBIDDEN`.
///
/// Wired at route-layer time, e.g.:
///
/// ```ignore
/// .route_layer(from_fn(move |req, next| {
///     require_api_key_scope(ApiKeyScope::MonitorRead, req, next)
/// }))
/// ```
pub async fn require_api_key_scope(
    scope: ApiKeyScope,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let scopes = request
        .extensions()
        .get::<ApiKeyScopes>()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !scopes.0.contains(&scope) {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(request).await)
}
