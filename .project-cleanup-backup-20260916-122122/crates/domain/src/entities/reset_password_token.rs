use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::{
    ids::{ResetPasswordTokenId, UserId},
    policies::Ownable,
};

/// A single-use password-reset token.
///
/// Mirrors `EmailVerificationToken`'s shape exactly (same "hash at rest,
/// plaintext prefix for lookup" pattern via `application::ports::PasswordHasher`
/// — see that type's doc comment for the full rationale). Kept as a
/// distinct entity/table rather than reusing `EmailVerificationToken`
/// because a reset token grants a materially more sensitive capability
/// (silently taking over the account by setting a brand-new password)
/// than an email-verification token does, and the two intentionally
/// carry different raw-token scheme prefixes (`rst_...` vs `evt_...`,
/// see `application::use_cases::reset_password_support`) so the two
/// kinds can never be confused for one another at the HTTP boundary.
#[derive(Clone, Serialize, Deserialize)]
pub struct ResetPasswordToken {
    pub id: ResetPasswordTokenId,
    pub user_id: UserId,
    pub token_prefix: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ResetPasswordToken {
    pub fn new(
        id: ResetPasswordTokenId,
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
    /// change via `ResetPasswordTokenRepository::mark_used` — this only
    /// updates the in-memory aggregate.
    pub fn mark_used(&mut self, when: DateTime<Utc>) {
        self.used_at = Some(when);
    }
}

impl Ownable for ResetPasswordToken {
    fn owner_id(&self) -> UserId {
        self.user_id
    }
}

// The `Debug` impls in this file sit after `mod tests` because they are
// the credential-redaction boundary added in PART 7 and were kept at the
// bottom to keep that PART's diff focused on the impls themselves. The
// ordering is a style preference; the lint is suppressed explicitly here
// rather than reorganising the file, so the diff stays minimal and the
// redaction review (PART 7) remains easy to re-audit.
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_token(expires_at: DateTime<Utc>) -> ResetPasswordToken {
        ResetPasswordToken::new(
            ResetPasswordTokenId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            "rst_abc123".to_string(),
            "argon2-hash".to_string(),
            expires_at,
            Utc::now(),
        )
    }

    #[test]
    fn new_token_starts_unused() {
        let token = make_token(Utc::now() + chrono::Duration::minutes(30));
        assert!(!token.is_used());
        assert!(token.used_at.is_none());
    }

    #[test]
    fn mark_used_sets_used_at() {
        let mut token = make_token(Utc::now() + chrono::Duration::minutes(30));
        let when = Utc::now();
        token.mark_used(when);
        assert!(token.is_used());
        assert_eq!(token.used_at, Some(when));
    }

    #[test]
    fn is_valid_true_when_unused_and_not_expired() {
        let token = make_token(Utc::now() + chrono::Duration::minutes(30));
        assert!(token.is_valid(Utc::now()));
    }

    #[test]
    fn is_valid_false_when_expired() {
        let token = make_token(Utc::now() - chrono::Duration::seconds(1));
        assert!(!token.is_valid(Utc::now()));
    }

    #[test]
    fn is_valid_false_when_used_even_if_not_expired() {
        let mut token = make_token(Utc::now() + chrono::Duration::minutes(30));
        token.mark_used(Utc::now());
        assert!(!token.is_valid(Utc::now()));
    }
}

/// PART 7 — manual Debug that redacts `token_prefix` and `token_hash`.
impl std::fmt::Debug for ResetPasswordToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResetPasswordToken")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("expires_at", &self.expires_at)
            .field("used_at", &self.used_at)
            .field("token_prefix", &"[REDACTED]")
            .field("token_hash", &"[REDACTED]")
            .finish()
    }
}
