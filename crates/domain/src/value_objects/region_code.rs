use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// Allowed region codes for distributed monitor checks.
///
/// The list is deliberately hard-coded and small: each region code maps
/// to a concrete agent deployment region. Adding a new region is a
/// deployment decision, not a runtime database edit — the application and
/// agent fleet must both understand the region before checks can actually
/// run there.
pub const ALLOWED_REGION_CODES: [&str; 4] = ["us-east", "eu-west", "af-west", "ap-southeast"];

/// A validated region code for a monitor check.
///
/// Mirrors `ProxyHost`/`Url` value-object shape: validated at
/// construction, stored as an owned `String`, exposed read-only via
/// `as_str()`/`Display`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionCode(String);

impl RegionCode {
    /// Validates a region code, lower-casing it first so `US-EAST` and
    /// `us-east` are accepted identically.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let normalized = value.into().trim().to_ascii_lowercase();

        if ALLOWED_REGION_CODES.contains(&normalized.as_str()) {
            Ok(Self(normalized))
        } else {
            Err(DomainError::UnsupportedRegionCode)
        }
    }

    /// Returns the default region code used for monitors that were
    /// created before multi-region checks existed.
    pub fn default_region() -> Self {
        Self("us-east".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for RegionCode {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for RegionCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_all_allowed_regions() {
        for region in ALLOWED_REGION_CODES {
            assert!(
                RegionCode::new(region).is_ok(),
                "expected {region} to be accepted"
            );
        }
    }

    #[test]
    fn lowercases_and_trims_input() {
        let region = RegionCode::new("  US-EAST  ").unwrap();
        assert_eq!(region.as_str(), "us-east");
    }

    #[test]
    fn rejects_unsupported_region() {
        assert_eq!(
            RegionCode::new("us-west"),
            Err(DomainError::UnsupportedRegionCode)
        );
    }

    #[test]
    fn default_region_is_us_east() {
        assert_eq!(RegionCode::default_region().as_str(), "us-east");
    }

    #[test]
    fn display_uses_lowercase_code() {
        let region = RegionCode::new("eu-west").unwrap();
        assert_eq!(format!("{region}"), "eu-west");
    }
}
