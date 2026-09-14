use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use domain::UserId;

use crate::{auth_state::AuthState, error::ApiError};

/// Axum middleware that requires the request to have a VALID, RECENT
/// MFA marker for the authenticated user.
///
/// Must be layered AFTER `jwt_auth` (which inserts `UserId` into
/// request extensions and rejects unauthenticated requests before this
/// even runs). Returns 403 with a "step-up authentication required"
/// body when no recent-MFA marker exists — i.e. the caller has a valid
/// JWT but has not completed a TOTP (or backup-code) verification
/// within `RECENT_MFA_TTL_SECONDS`.
///
/// SECURITY INVARIANT: possession of a normal JWT alone must never be
/// sufficient for platform-wide financial administration. This
/// middleware enforces that at the HTTP layer; the per-handler
/// `require_platform_admin` checks only enforce "is a platform
/// admin", not "has recently proved possession of a second factor".
///
/// Every failure mode (missing user id, cache read error, absent
/// marker) fails CLOSED — the caller gets a 4xx, never an accidental
/// bypass.
pub async fn require_recent_mfa(
    State(auth): State<AuthState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let user_id = *request
        .extensions()
        .get::<UserId>()
        .ok_or_else(|| ApiError::unauthorized("authentication required").into_response())?;

    let key = application::recent_mfa_cache_key(user_id);
    let marker = auth.pending_auth_cache.get(&key).await.map_err(|_| {
        ApiError::internal_with_source(
            "recent-mfa cache unavailable",
            "PendingAuthCache::get failed",
        )
        .into_response()
    })?;

    if marker.is_none() {
        return Err(ApiError::forbidden(
            "step-up authentication required: please verify MFA and retry",
        )
        .into_response());
    }

    Ok(next.run(request).await)
}
