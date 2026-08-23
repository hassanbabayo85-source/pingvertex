use domain::UserId;

/// TTL (seconds) for a user's "session invalidated before" marker in
/// `PendingAuthCache` (Redis in production — see `infra::RedisPendingAuthCache`).
///
/// Matches the maximum lifetime of a session JWT (24 hours — see
/// `JsonWebTokenService::generate`). Once a JWT's own `exp` has passed it
/// is rejected on that basis alone regardless of this marker, so the
/// marker itself never needs to outlive the longest-lived token that
/// could still be presented — keeping this TTL matched to that lifetime
/// stops the Redis key from growing unbounded / needing manual cleanup.
pub const SESSION_INVALIDATION_TTL_SECONDS: u64 = 86400;

/// Redis (via `PendingAuthCache`) key holding the unix timestamp
/// (seconds) before which every session JWT issued to `user_id` must be
/// treated as revoked.
///
/// Written by `ResetPasswordUseCase` the moment a password reset
/// succeeds (B6) — available for any future flow that needs to force a
/// logout (e.g. a "log out all other devices" account-settings action).
/// Read by `jwt_auth` (`crates/api/src/middleware/jwt_auth.rs`) on every
/// authenticated request: a token whose own `iat` (see
/// `application::ports::VerifiedToken`) predates this marker is
/// rejected, even though its signature and `exp` are still valid.
pub fn session_invalidated_at_cache_key(user_id: UserId) -> String {
    format!("session_invalidated_at:{user_id}")
}
