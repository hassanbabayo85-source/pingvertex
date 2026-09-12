use crate::errors::DomainError;

/// Hard cap on the byte length of a monitor URL.
///
/// Without this limit, a client could submit a URL string thousands (or
/// millions) of characters long. `::url::Url::parse` would still attempt to
/// parse the whole thing, and the value would flow into validation/storage
/// downstream — wasted CPU and database work for a value no legitimate
/// monitor URL would ever need. 2048 comfortably covers real-world URLs
/// (browsers commonly cap addressable URLs around this size) while
/// rejecting abusive payloads outright.
const MAX_URL_LENGTH: usize = 2048;

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

        // Checked before parsing so an oversized string never reaches the
        // (comparatively expensive) URL parser.
        if value.len() > MAX_URL_LENGTH {
            return Err(DomainError::UrlTooLong);
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

    #[test]
    fn rejects_url_longer_than_2048_characters() {
        let long_path = "a".repeat(2100);
        let long_url = format!("https://example.com/{long_path}");

        assert_eq!(Url::new(long_url), Err(DomainError::UrlTooLong));
    }

    #[test]
    fn accepts_url_at_2048_character_boundary() {
        // Build a URL whose total length is exactly 2048 bytes.
        let prefix = "https://example.com/";
        let padding_len = 2048 - prefix.len();
        let url_string = format!("{prefix}{}", "a".repeat(padding_len));
        assert_eq!(url_string.len(), 2048);

        assert!(Url::new(url_string).is_ok());
    }
}
