use domain::UserId;

/// TTL (seconds) for a user's "session invalidated before" marker in
/// `PendingAuthCache` (Redis in production).
///
/// This must cover the maximum lifetime of a session JWT. Session JWTs are
/// currently valid for 24 hours, so the marker is retained for the same
/// period.
pub const SESSION_INVALIDATION_TTL_SECONDS: u64 = 86400;

/// Redis key holding the unix timestamp (seconds) at/before which every
/// session JWT issued for `user_id` must be treated as revoked.
pub fn session_invalidated_at_cache_key(user_id: UserId) -> String {
    format!("session_invalidated_at:{user_id}")
}

/// Returns a revocation timestamp that also invalidates tokens issued in the
/// same second as the logout request. JWT `iat` is second-granularity and the
/// middleware rejects tokens where `iat < invalidated_at`, so `now + 1` is
/// intentional here.
pub fn session_revocation_timestamp(now_unix_seconds: i64) -> i64 {
    now_unix_seconds.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_timestamp_invalidates_tokens_from_same_second() {
        assert_eq!(session_revocation_timestamp(1_000), 1_001);
    }

    #[test]
    fn revocation_timestamp_does_not_overflow() {
        assert_eq!(session_revocation_timestamp(i64::MAX), i64::MAX);
    }
}
