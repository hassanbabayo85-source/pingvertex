use std::error::Error;

/// How long a password-reset token stays valid.
///
/// Deliberately much shorter than `EMAIL_VERIFICATION_TOKEN_TTL_HOURS`
/// (24h): a reset token grants the ability to take over the account
/// outright (set a brand new password), so it carries a tighter exposure
/// window than a mere "prove you own this inbox" email-verification
/// token. 30 minutes is generous enough for a real user to click an
/// email link, short enough that a leaked/intercepted link has a small
/// blast radius.
pub const RESET_PASSWORD_TOKEN_TTL_MINUTES: i64 = 30;

/// Extracts the plaintext lookup prefix from a raw password-reset token.
///
/// Raw token format: `rst_<16 hex chars>_<64 hex chars>`.
/// The stored prefix is `rst_<16 hex chars>`.
pub fn reset_password_token_prefix(
    raw_token: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut parts = raw_token.splitn(3, '_');
    let scheme = parts.next().unwrap_or("");
    let prefix_part = parts.next().unwrap_or("");
    let secret_part = parts.next().ok_or("invalid reset token")?;

    if parts.next().is_some() {
        return Err("invalid reset token".into());
    }

    if scheme != "rst"
        || prefix_part.len() != 16
        || secret_part.len() != 64
        || !prefix_part.chars().all(|c| c.is_ascii_hexdigit())
        || !secret_part.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("invalid reset token".into());
    }

    Ok(format!("rst_{prefix_part}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_valid_prefix() {
        let raw = format!("rst_{}_", "a".repeat(16)) + &"b".repeat(64);
        assert_eq!(
            reset_password_token_prefix(&raw).unwrap(),
            "rst_aaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn rejects_malformed_token() {
        assert!(reset_password_token_prefix("bad_short").is_err());
        assert!(reset_password_token_prefix("rst_abcd_short").is_err());
    }

    #[test]
    fn rejects_non_hex_prefix() {
        let raw = format!("rst_{}_{}", "g".repeat(16), "h".repeat(64));
        assert!(reset_password_token_prefix(&raw).is_err());
    }

    #[test]
    fn rejects_email_verification_scheme_tag() {
        // An `evt_...` token must never be accepted as a `rst_...` one,
        // even though the shape (hex lengths) is otherwise identical.
        let raw = format!("evt_{}_{}", "a".repeat(16), "b".repeat(64));
        assert!(reset_password_token_prefix(&raw).is_err());
    }
}
