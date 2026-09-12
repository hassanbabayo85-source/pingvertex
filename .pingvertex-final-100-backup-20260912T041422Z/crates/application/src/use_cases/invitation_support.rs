use std::error::Error;

/// How long an invitation stays valid before it expires.
pub const INVITATION_TOKEN_TTL_HOURS: i64 = 168;

/// Extracts the plaintext lookup prefix from a raw invitation token.
///
/// Raw token format: `inv_<16 hex chars>_<64 hex chars>`.
/// The stored prefix is `inv_<16 hex chars>`.
pub fn invitation_token_prefix(raw_token: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut parts = raw_token.splitn(3, '_');
    let scheme = parts.next().unwrap_or("");
    let prefix_part = parts.next().unwrap_or("");
    let secret_part = parts.next().ok_or("invalid invitation token")?;

    if parts.next().is_some() {
        return Err("invalid invitation token".into());
    }

    if scheme != "inv"
        || prefix_part.len() != 16
        || secret_part.len() != 64
        || !prefix_part.chars().all(|c| c.is_ascii_hexdigit())
        || !secret_part.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("invalid invitation token".into());
    }

    Ok(format!("inv_{prefix_part}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_valid_prefix() {
        let raw = format!("inv_{}_", "a".repeat(16)) + &"b".repeat(64);
        assert_eq!(
            invitation_token_prefix(&raw).unwrap(),
            "inv_aaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn rejects_malformed_token() {
        assert!(invitation_token_prefix("bad_short").is_err());
        assert!(invitation_token_prefix("inv_abcd_short").is_err());
    }

    #[test]
    fn rejects_non_hex_prefix() {
        let raw = format!("inv_{}_{}", "g".repeat(16), "h".repeat(64));
        assert!(invitation_token_prefix(&raw).is_err());
    }

    #[test]
    fn rejects_email_verification_scheme_tag() {
        let raw = format!("evt_{}_{}", "a".repeat(16), "b".repeat(64));
        assert!(invitation_token_prefix(&raw).is_err());
    }
}
