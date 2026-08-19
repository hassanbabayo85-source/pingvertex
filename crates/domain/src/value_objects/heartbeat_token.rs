use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::DomainError;

/// A high-entropy heartbeat token used to authenticate `GET /heartbeat/{token}`
/// pings for `MonitorKind::Heartbeat` monitors.
///
/// Mirrors `RegionCode`/`ProxyHost`'s value-object shape: validated at
/// construction, stored as an owned `String`, exposed read-only via
/// `as_str()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatToken(String);

impl HeartbeatToken {
    /// Generates a new high-entropy heartbeat token (UUID v4).
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Validates a provided heartbeat token. Accepts a UUID (any version,
    /// since `generate()` always produces v4 but we don't want to reject a
    /// manually-supplied token purely on version bits) or a 32-character
    /// lowercase hex string.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();

        if value.is_empty() {
            return Err(DomainError::InvalidHeartbeatToken);
        }

        // Canonical hyphenated UUID form (any version/case is fine here —
        // `generate()` always produces lowercase v4, but a manually
        // supplied UUID-shaped token is still a valid high-entropy token).
        if value.contains('-') {
            if Uuid::parse_str(value).is_ok() {
                return Ok(Self(value.to_string()));
            }
            return Err(DomainError::InvalidHeartbeatToken);
        }

        // Bare 32-character lowercase hex form (no hyphens).
        if value.len() == 32
            && value.chars().all(|c| c.is_ascii_hexdigit())
            && !value.chars().any(|c| c.is_ascii_uppercase())
        {
            return Ok(Self(value.to_string()));
        }

        Err(DomainError::InvalidHeartbeatToken)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for HeartbeatToken {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_a_valid_token() {
        let token = HeartbeatToken::generate();
        assert!(HeartbeatToken::new(token.as_str()).is_ok());
    }

    #[test]
    fn accepts_valid_uuid() {
        assert!(HeartbeatToken::new("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn accepts_32_char_lowercase_hex() {
        assert!(HeartbeatToken::new("0123456789abcdef0123456789abcdef").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(
            HeartbeatToken::new(""),
            Err(DomainError::InvalidHeartbeatToken)
        );
    }

    #[test]
    fn rejects_uppercase_hex() {
        assert_eq!(
            HeartbeatToken::new("0123456789ABCDEF0123456789ABCDEF"),
            Err(DomainError::InvalidHeartbeatToken)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            HeartbeatToken::new("not-a-token"),
            Err(DomainError::InvalidHeartbeatToken)
        );
    }
}
