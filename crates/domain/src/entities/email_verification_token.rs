use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::{
    ids::{EmailVerificationTokenId, UserId},
    policies::Ownable,
};

/// A single-use email verification token.
///
/// Mirrors `BackupCode`'s "hash at rest" shape: the raw token is shown to
/// the user exactly once (embedded in the verification email) — only its
/// Argon2id hash (`token_hash`, via `application::ports::PasswordHasher`,
/// the SAME hasher used for account passwords and backup codes) is ever
/// persisted.
///
/// UNLIKE `BackupCode`, a verification token also carries a plaintext
/// `token_prefix` (see `IdGenerator::next_email_verification_token`'s doc
/// comment for the exact format). This is the SAME "prefix stored in the
/// clear, secret Argon2-hashed" pattern this codebase already uses for
/// `ApiKey` — necessary because Argon2's random salt makes a hash
/// non-deterministic, so a raw token presented at `POST
/// /auth/verify-email` cannot be looked up directly by `WHERE token_hash
/// = hash(raw)`. The prefix gives an indexed O(1) lookup
/// (`EmailVerificationTokenRepository::find_by_prefix`); the full raw
/// token is still re-verified against `token_hash` afterward before the
/// token is accepted, so a prefix match alone is never sufficient.
///
/// A token is single-use: `mark_used` records `used_at` the moment it is
/// redeemed by `VerifyEmailUseCase` — every lookup path must treat a
/// token with `used_at.is_some()`, or `expires_at` in the past, as no
/// longer valid (see `is_valid`).
#[derive(Clone, Serialize, Deserialize)]
pub struct EmailVerificationToken {
    pub id: EmailVerificationTokenId,
    pub user_id: UserId,
    pub token_prefix: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl EmailVerificationToken {
    pub fn new(
        id: EmailVerificationTokenId,
        user_id: UserId,
        token_prefix: String,
        token_hash: String,
        expires_at: DateTime<Utc>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            user_id,
            token_prefix,
            token_hash,
            expires_at,
            used_at: None,
            created_at,
        }
    }

    pub fn is_used(&self) -> bool {
        self.used_at.is_some()
    }

    /// `true` only if the token has never been redeemed AND has not yet
    /// expired as of `now`.
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        !self.is_used() && self.expires_at > now
    }

    /// Marks this token as redeemed. Callers must also persist the
    /// change via `EmailVerificationTokenRepository::mark_used` — this
    /// only updates the in-memory aggregate.
    pub fn mark_used(&mut self, when: DateTime<Utc>) {
        self.used_at = Some(when);
    }
}

impl Ownable for EmailVerificationToken {
    fn owner_id(&self) -> UserId {
        self.user_id
    }
}

// The `Debug` impls in this file sit after `mod tests` because they are
// the credential-redaction boundary added and were kept at the
// bottom to keep that PART's diff focused on the impls themselves. The
// ordering is a style preference; the lint is suppressed explicitly here
// rather than reorganising the file, so the diff stays minimal and the
// redaction review remains easy to re-audit.
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_token(expires_at: DateTime<Utc>) -> EmailVerificationToken {
        EmailVerificationToken::new(
            EmailVerificationTokenId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            "evt_abc123".to_string(),
            "argon2-hash".to_string(),
            expires_at,
            Utc::now(),
        )
    }

    #[test]
    fn new_token_starts_unused() {
        let token = make_token(Utc::now() + chrono::Duration::hours(1));
        assert!(!token.is_used());
        assert!(token.used_at.is_none());
    }

    #[test]
    fn mark_used_sets_used_at() {
        let mut token = make_token(Utc::now() + chrono::Duration::hours(1));
        let when = Utc::now();
        token.mark_used(when);
        assert!(token.is_used());
        assert_eq!(token.used_at, Some(when));
    }

    #[test]
    fn is_valid_true_when_unused_and_not_expired() {
        let token = make_token(Utc::now() + chrono::Duration::hours(1));
        assert!(token.is_valid(Utc::now()));
    }

    #[test]
    fn is_valid_false_when_expired() {
        let token = make_token(Utc::now() - chrono::Duration::seconds(1));
        assert!(!token.is_valid(Utc::now()));
    }

    #[test]
    fn is_valid_false_when_used_even_if_not_expired() {
        let mut token = make_token(Utc::now() + chrono::Duration::hours(1));
        token.mark_used(Utc::now());
        assert!(!token.is_valid(Utc::now()));
    }
}

/// Manual Debug that redacts `token_prefix` and `token_hash`.
impl std::fmt::Debug for EmailVerificationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailVerificationToken")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("expires_at", &self.expires_at)
            .field("used_at", &self.used_at)
            .field("token_prefix", &"[REDACTED]")
            .field("token_hash", &"[REDACTED]")
            .finish()
    }
}
