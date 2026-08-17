use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// A validated 6-digit hex color, e.g. `#1A2B3C`.
///
/// Must match `^#[0-9A-Fa-f]{6}$` exactly. Stored normalized to
/// uppercase hex digits (with the leading `#`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HexColor(String);

impl HexColor {
    /// Creates a validated hex color.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();

        let bytes = value.as_bytes();

        if bytes.len() != 7 || bytes[0] != b'#' {
            return Err(DomainError::InvalidHexColor);
        }

        if !bytes[1..].iter().all(|b| b.is_ascii_hexdigit()) {
            return Err(DomainError::InvalidHexColor);
        }

        Ok(Self(value.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for HexColor {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_hex_color() {
        assert_eq!(HexColor::new("#1a2b3c").unwrap().as_str(), "#1A2B3C");
    }

    #[test]
    fn accepts_uppercase() {
        assert!(HexColor::new("#ABCDEF").is_ok());
    }

    #[test]
    fn rejects_missing_hash() {
        assert_eq!(HexColor::new("1a2b3c"), Err(DomainError::InvalidHexColor));
    }

    #[test]
    fn rejects_short_hex() {
        assert_eq!(HexColor::new("#fff"), Err(DomainError::InvalidHexColor));
    }

    #[test]
    fn rejects_long_hex() {
        assert_eq!(
            HexColor::new("#1a2b3c4d"),
            Err(DomainError::InvalidHexColor)
        );
    }

    #[test]
    fn rejects_non_hex_characters() {
        assert_eq!(HexColor::new("#gggggg"), Err(DomainError::InvalidHexColor));
    }

    #[test]
    fn rejects_injection_attempt() {
        // Defends the downstream consumer (CSS/HTML rendering of the
        // public status page) — a value like this must never pass
        // validation and reach the template.
        assert_eq!(
            HexColor::new("#123456;background:url(x)"),
            Err(DomainError::InvalidHexColor)
        );
    }
}
