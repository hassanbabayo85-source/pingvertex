use url::Url;

use crate::errors::DomainError;

/// A validated Slack incoming-webhook URL.
///
/// Domain must be exactly `hooks.slack.com`. No arbitrary URLs are allowed
/// — this is a domain-layer pre-filter. The actual SSRF/DNS-rebinding
/// protection happens at send time in `SlackNotificationService`, using
/// `shared::ssrf::resolve_and_validate_destination`, exactly like the
/// existing `WebhookNotificationService`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackWebhookUrl(String);

impl SlackWebhookUrl {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();

        if value.is_empty() {
            return Err(DomainError::EmptyUrl);
        }

        let parsed = Url::parse(value).map_err(|_| DomainError::InvalidUrl)?;

        if parsed.scheme() != "https" {
            return Err(DomainError::UnsupportedUrlScheme);
        }

        if parsed.host_str() != Some("hooks.slack.com") {
            return Err(DomainError::InvalidSlackWebhookUrl);
        }

        Ok(Self(parsed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for SlackWebhookUrl {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_slack_webhook_url() {
        assert!(SlackWebhookUrl::new("https://hooks.slack.com/services/T000/B000/XXXX").is_ok());
    }

    #[test]
    fn rejects_non_slack_domain() {
        assert_eq!(
            SlackWebhookUrl::new("https://example.com/hooks/123"),
            Err(DomainError::InvalidSlackWebhookUrl)
        );
    }

    #[test]
    fn rejects_http_scheme() {
        assert_eq!(
            SlackWebhookUrl::new("http://hooks.slack.com/services/123"),
            Err(DomainError::UnsupportedUrlScheme)
        );
    }
}
