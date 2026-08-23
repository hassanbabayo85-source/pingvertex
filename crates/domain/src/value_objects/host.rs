use crate::errors::DomainError;

/// Hard cap on the byte length of a hostname/IP target for TCP and DNS
/// monitors. 253 is the DNS max label length; it comfortably covers any
/// legitimate hostname or IPv4/IPv6 literal.
const MAX_HOST_LENGTH: usize = 253;

/// A validated host target (domain name or IP address) for TCP and DNS
/// monitors. Distinct from `Url` — this must NOT be a full URL with a
/// scheme. For HTTP/HTTPS monitors the `Url` value object remains the
/// correct validation shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host(String);

impl Host {
    /// Creates a validated host.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();

        if value.is_empty() {
            return Err(DomainError::EmptyHost);
        }

        if value.len() > MAX_HOST_LENGTH {
            return Err(DomainError::HostTooLong);
        }

        // A host target must not contain a URL scheme. Anything with
        // `://` is a URL, not a bare host, and must be rejected here so
        // TCP/DNS monitors don't silently store a full URL as their host.
        if value.contains("://") {
            return Err(DomainError::InvalidHost);
        }

        // Disallow whitespace inside the host — trim only removes the
        // ends, not internal spaces which are never valid in a host.
        if value.chars().any(char::is_whitespace) {
            return Err(DomainError::InvalidHost);
        }

        Ok(Self(value.to_owned()))
    }

    /// Returns the host string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Host {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_domain() {
        let host = Host::new("db.example.com").unwrap();
        assert_eq!(host.as_str(), "db.example.com");
    }

    #[test]
    fn accepts_ipv4() {
        assert!(Host::new("192.168.1.1").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Host::new(""), Err(DomainError::EmptyHost));
    }

    #[test]
    fn rejects_url_scheme() {
        assert_eq!(
            Host::new("https://example.com"),
            Err(DomainError::InvalidHost)
        );
    }

    #[test]
    fn rejects_internal_whitespace() {
        assert_eq!(Host::new("db host"), Err(DomainError::InvalidHost));
    }

    #[test]
    fn rejects_too_long() {
        let value = "a".repeat(254);
        assert_eq!(Host::new(value), Err(DomainError::HostTooLong));
    }
}
