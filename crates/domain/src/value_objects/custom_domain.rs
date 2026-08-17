use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// Hard cap on custom domain length — matches the practical max length of
/// a fully-qualified DNS hostname (see `ProxyHost`'s identical rationale).
const MAX_CUSTOM_DOMAIN_LENGTH: usize = 253;

fn is_valid_domain_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '-'
}

/// A validated custom domain a customer wants to point at their status
/// page, e.g. `status.acme.com`.
///
/// IMPORTANT — this type only validates the *syntax* of the domain. It
/// says NOTHING about whether the caller actually owns it. Ownership is
/// established out-of-band via a DNS TXT record challenge (see
/// `StatusPage::request_custom_domain` / `StatusPage::verify_custom_domain`
/// in `entities::StatusPage`), and MUST be confirmed before this domain is
/// used for host-header routing anywhere in the API layer. Constructing a
/// `CustomDomain` value is not, by itself, proof of ownership and must
/// never be treated as such.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomDomain(String);

impl CustomDomain {
    /// Creates a validated custom domain (syntax only — see type docs).
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim().to_ascii_lowercase();

        if value.is_empty() {
            return Err(DomainError::EmptyCustomDomain);
        }

        if value.len() > MAX_CUSTOM_DOMAIN_LENGTH {
            return Err(DomainError::CustomDomainTooLong);
        }

        if !value.chars().all(is_valid_domain_char) {
            return Err(DomainError::InvalidCustomDomain);
        }

        if value.starts_with('.')
            || value.ends_with('.')
            || value.starts_with('-')
            || value.ends_with('-')
            || value.contains("..")
        {
            return Err(DomainError::InvalidCustomDomain);
        }

        // Must contain at least one dot (a bare label like "localhost" is
        // not a valid custom domain for this feature).
        if !value.contains('.') {
            return Err(DomainError::InvalidCustomDomain);
        }

        // Each DNS label must independently respect the 63-char limit and
        // must not start/end with a hyphen.
        for label in value.split('.') {
            if label.is_empty() || label.len() > 63 {
                return Err(DomainError::InvalidCustomDomain);
            }
            if label.starts_with('-') || label.ends_with('-') {
                return Err(DomainError::InvalidCustomDomain);
            }
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CustomDomain {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_domain() {
        assert_eq!(
            CustomDomain::new("status.acme.com").unwrap().as_str(),
            "status.acme.com"
        );
    }

    #[test]
    fn lowercases_input() {
        assert_eq!(
            CustomDomain::new("Status.Acme.COM").unwrap().as_str(),
            "status.acme.com"
        );
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(CustomDomain::new(""), Err(DomainError::EmptyCustomDomain));
    }

    #[test]
    fn rejects_bare_label_without_dot() {
        assert_eq!(
            CustomDomain::new("localhost"),
            Err(DomainError::InvalidCustomDomain)
        );
    }

    #[test]
    fn rejects_double_dot() {
        assert_eq!(
            CustomDomain::new("status..acme.com"),
            Err(DomainError::InvalidCustomDomain)
        );
    }

    #[test]
    fn rejects_leading_trailing_dot() {
        assert_eq!(
            CustomDomain::new(".status.acme.com"),
            Err(DomainError::InvalidCustomDomain)
        );
        assert_eq!(
            CustomDomain::new("status.acme.com."),
            Err(DomainError::InvalidCustomDomain)
        );
    }

    #[test]
    fn rejects_label_starting_or_ending_with_hyphen() {
        assert_eq!(
            CustomDomain::new("-status.acme.com"),
            Err(DomainError::InvalidCustomDomain)
        );
        assert_eq!(
            CustomDomain::new("status-.acme.com"),
            Err(DomainError::InvalidCustomDomain)
        );
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(
            CustomDomain::new("status.acme.com/evil"),
            Err(DomainError::InvalidCustomDomain)
        );
        assert_eq!(
            CustomDomain::new("status.acme.com; DROP TABLE"),
            Err(DomainError::InvalidCustomDomain)
        );
    }

    #[test]
    fn rejects_longer_than_253_characters() {
        let long_domain = format!("{}.acme.com", "a".repeat(250));
        assert!(long_domain.len() > 253);
        assert_eq!(
            CustomDomain::new(long_domain),
            Err(DomainError::CustomDomainTooLong)
        );
    }
}
