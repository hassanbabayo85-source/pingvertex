use domain::UserId;

/// TTL for a "recent MFA" marker, in seconds.
///
/// Five minutes is the step-up window for high-risk platform-admin
/// operations: after a successful TOTP verification, an admin may
/// perform high-risk operations for this long before being challenged
/// again. Chosen to be long enough to complete a multi-step admin
/// task (e.g. reviewing then suspending a company), but short enough
/// that a stolen session cookie cannot be used to perform irreversible
/// financial operations hours later.
///
/// This is what the `require_recent_mfa` API middleware checks, and
/// what the `VerifyTotpAndLoginUseCase`/`ConfirmTotpSetupUseCase`
/// write. See also `application::session_invalidated_at_cache_key`
/// for the analogous (longer-lived) session-revocation marker.
pub const RECENT_MFA_TTL_SECONDS: u64 = 300;

/// Redis (`PendingAuthCache`) key holding the "recently verified MFA"
/// marker for a user.
///
/// Presence of this key (with a non-expired TTL) is what
/// `require_recent_mfa` checks. The marker's VALUE is the Unix
/// timestamp of the successful verification, kept for
/// observability/debugging — never trusted for the freshness check
/// itself, which relies on the cache's own TTL.
pub fn recent_mfa_cache_key(user_id: UserId) -> String {
    format!("recent_mfa:{user_id}")
}
