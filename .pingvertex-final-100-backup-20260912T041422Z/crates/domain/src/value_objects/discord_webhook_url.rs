use url::Url;

use crate::errors::DomainError;

/// A validated Discord webhook URL.
///
/// Domain must be `discord.com` and path must start with `/api/webhooks/`.
/// The same domain/path pre-filter as `SlackWebhookUrl`, with send-time
/// SSRF protection in `DiscordNotificationService`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordWebhookUrl(String);

impl DiscordWebhookUrl {
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

        if parsed.host_str() != Some("discord.com") {
            return Err(DomainError::InvalidDiscordWebhookUrl);
        }

        if !parsed.path().starts_with("/api/webhooks/") {
            return Err(DomainError::InvalidDiscordWebhookUrl);
        }

        Ok(Self(parsed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for DiscordWebhookUrl {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_discord_webhook_url() {
        assert!(DiscordWebhookUrl::new("https://discord.com/api/webhooks/123456/abcdef").is_ok());
    }

    #[test]
    fn rejects_wrong_path() {
        assert_eq!(
            DiscordWebhookUrl::new("https://discord.com/api/v2/webhooks/123"),
            Err(DomainError::InvalidDiscordWebhookUrl)
        );
    }

    #[test]
    fn rejects_other_domain() {
        assert_eq!(
            DiscordWebhookUrl::new("https://discordapp.com/api/webhooks/123"),
            Err(DomainError::InvalidDiscordWebhookUrl)
        );
    }
}
