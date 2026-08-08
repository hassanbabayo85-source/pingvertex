use crate::errors::DomainError;

/// A validated HTTP or HTTPS URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url(String);

impl Url {
    /// Creates a validated URL.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();

        if value.is_empty() {
            return Err(DomainError::EmptyUrl);
        }

        let parsed = ::url::Url::parse(value).map_err(|_| DomainError::InvalidUrl)?;

        match parsed.scheme() {
            "http" | "https" => {}
            _ => return Err(DomainError::UnsupportedUrlScheme),
        }

        Ok(Self(parsed.to_string()))
    }

    /// Returns the URL as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Url {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https() {
        let url = Url::new("https://example.com").unwrap();

        assert_eq!(url.as_str(), "https://example.com/");
    }

    #[test]
    fn accepts_http() {
        assert!(Url::new("http://localhost:8080").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Url::new(""), Err(DomainError::EmptyUrl));
    }

    #[test]
    fn rejects_invalid_url() {
        assert_eq!(Url::new("example"), Err(DomainError::InvalidUrl));
    }

    #[test]
    fn rejects_unsupported_scheme() {
        assert_eq!(
            Url::new("ftp://example.com"),
            Err(DomainError::UnsupportedUrlScheme)
        );
    }
}
