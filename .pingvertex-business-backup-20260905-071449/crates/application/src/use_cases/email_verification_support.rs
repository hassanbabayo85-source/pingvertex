use std::error::Error;

/// How long a verification token stays valid.
pub const EMAIL_VERIFICATION_TOKEN_TTL_HOURS: i64 = 24;

/// Extracts the plaintext lookup prefix from a raw verification token.
///
/// Raw token format: `evt_<16 hex chars>_<64 hex chars>`.
/// The stored prefix is `evt_<16 hex chars>`.
pub fn email_verification_token_prefix(
    raw_token: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut parts = raw_token.splitn(3, '_');
    let scheme = parts.next().unwrap_or("");
    let prefix_part = parts.next().unwrap_or("");
    let secret_part = parts.next().ok_or("invalid verification token")?;

    if parts.next().is_some() {
        return Err("invalid verification token".into());
    }

    if scheme != "evt"
        || prefix_part.len() != 16
        || secret_part.len() != 64
        || !prefix_part.chars().all(|c| c.is_ascii_hexdigit())
        || !secret_part.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("invalid verification token".into());
    }

    Ok(format!("evt_{prefix_part}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_valid_prefix() {
        let raw = format!("evt_{}_", "a".repeat(16)) + &"b".repeat(64);
        assert_eq!(
            email_verification_token_prefix(&raw).unwrap(),
            "evt_aaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn rejects_malformed_token() {
        assert!(email_verification_token_prefix("bad_short").is_err());
        assert!(email_verification_token_prefix("evt_abcd_short").is_err());
    }

    #[test]
    fn rejects_non_hex_prefix() {
        let raw = format!("evt_{}_{}", "g".repeat(16), "h".repeat(64));
        assert!(email_verification_token_prefix(&raw).is_err());
    }
}
