use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// Subdomains that are permanently forbidden because they collide with
/// existing/likely-future platform routes, or are common phishing targets
/// (e.g. an attacker registering `admin.pingvertex.com` to look official).
///
/// This list is intentionally an ALLOWLIST-STYLE denylist checked against
/// the FULL set of reserved words below — not a substring/prefix check —
/// so a legitimate customer name like "apiary" or "wwwatch" is never
/// wrongly rejected.
const RESERVED_SUBDOMAINS: &[&str] = &[
    "api",
    "www",
    "admin",
    "app",
    "dashboard",
    "status",
    "mail",
    "smtp",
    "ftp",
    "ns1",
    "ns2",
    "root",
    "support",
    "help",
    "billing",
    "auth",
    "login",
    "register",
    "docs",
    "blog",
    "cdn",
    "static",
    "assets",
    "staging",
    "dev",
    "test",
    "internal",
    "metrics",
    "grafana",
    "prometheus",
    "vpn",
    "webhook",
    "webhooks",
    "public",
];

/// A validated status page subdomain, e.g. `acme` in `acme.pingvertex.com`.
///
/// Rules (RFC 1035 DNS label rules, tightened for our use case):
/// - Cannot be empty.
/// - Max 63 characters (the DNS label limit).
/// - Only lowercase ASCII alphanumeric characters and hyphens.
/// - Cannot start or end with a hyphen.
/// - Cannot be a reserved word (see `RESERVED_SUBDOMAINS`).
///
/// Stored and compared in lowercase, since DNS labels are
/// case-insensitive and mixed-case input would otherwise let two
/// "different" subdomains collide at the DNS/HTTP layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Subdomain(String);

impl Subdomain {
    pub const MAX_LEN: usize = 63;

    /// Creates a validated subdomain.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim().to_ascii_lowercase();

        if value.is_empty() {
            return Err(DomainError::EmptySubdomain);
        }

        if value.chars().count() > Self::MAX_LEN {
            return Err(DomainError::SubdomainTooLong);
        }

        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(DomainError::InvalidSubdomainCharacters);
        }

        if value.starts_with('-') || value.ends_with('-') {
            return Err(DomainError::InvalidSubdomainCharacters);
        }

        if RESERVED_SUBDOMAINS.contains(&value.as_str()) {
            return Err(DomainError::ReservedSubdomain);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Subdomain {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_subdomain() {
        assert_eq!(Subdomain::new("acme").unwrap().as_str(), "acme");
    }

    #[test]
    fn lowercases_input() {
        assert_eq!(Subdomain::new("Acme").unwrap().as_str(), "acme");
    }

    #[test]
    fn accepts_hyphens_in_middle() {
        assert!(Subdomain::new("acme-corp").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Subdomain::new(""), Err(DomainError::EmptySubdomain));
        assert_eq!(Subdomain::new("   "), Err(DomainError::EmptySubdomain));
    }

    #[test]
    fn rejects_longer_than_63_chars() {
        let value = "a".repeat(64);
        assert_eq!(Subdomain::new(value), Err(DomainError::SubdomainTooLong));
    }

    #[test]
    fn accepts_63_chars() {
        let value = "a".repeat(63);
        assert!(Subdomain::new(value).is_ok());
    }

    #[test]
    fn rejects_leading_hyphen() {
        assert_eq!(
            Subdomain::new("-acme"),
            Err(DomainError::InvalidSubdomainCharacters)
        );
    }

    #[test]
    fn rejects_trailing_hyphen() {
        assert_eq!(
            Subdomain::new("acme-"),
            Err(DomainError::InvalidSubdomainCharacters)
        );
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(
            Subdomain::new("acme_corp"),
            Err(DomainError::InvalidSubdomainCharacters)
        );
        assert_eq!(
            Subdomain::new("acme.corp"),
            Err(DomainError::InvalidSubdomainCharacters)
        );
        assert_eq!(
            Subdomain::new("acme corp"),
            Err(DomainError::InvalidSubdomainCharacters)
        );
    }

    #[test]
    fn rejects_reserved_subdomains() {
        for reserved in ["api", "www", "admin", "status", "internal"] {
            assert_eq!(
                Subdomain::new(reserved),
                Err(DomainError::ReservedSubdomain),
                "expected {reserved} to be rejected"
            );
        }
    }

    #[test]
    fn reserved_check_is_exact_not_substring() {
        // "apiary" contains "api" as a prefix but must NOT be rejected —
        // the reserved check must match the whole label, not a substring.
        assert!(Subdomain::new("apiary").is_ok());
    }
}
