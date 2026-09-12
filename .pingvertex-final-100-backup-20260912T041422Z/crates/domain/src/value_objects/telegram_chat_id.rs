use crate::errors::DomainError;

/// A validated Telegram chat ID.
///
/// Telegram chat IDs are integers: positive for private (user) chats,
/// negative for groups/supergroups (e.g. `-1001234567890`). Stored and
/// transmitted as a string since that's the format Telegram's
/// `sendMessage` API and our JSON transport both use, but validated here
/// to reject anything that isn't optionally-negative digits.
///
/// This is per-channel data (unlike the bot token, it isn't a secret) —
/// each notification channel targets its own chat, which is why it lives
/// on `NotificationChannel::target` rather than a shared server-wide
/// default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramChatId(String);

impl TelegramChatId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();

        if value.is_empty() {
            return Err(DomainError::InvalidTelegramChatId);
        }

        let digits = value.strip_prefix('-').unwrap_or(value);

        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(DomainError::InvalidTelegramChatId);
        }

        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TelegramChatId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_positive_chat_id() {
        assert!(TelegramChatId::new("123456789").is_ok());
    }

    #[test]
    fn accepts_negative_group_chat_id() {
        assert!(TelegramChatId::new("-1001234567890").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(
            TelegramChatId::new(""),
            Err(DomainError::InvalidTelegramChatId)
        );
    }

    #[test]
    fn rejects_non_numeric() {
        assert_eq!(
            TelegramChatId::new("abc123"),
            Err(DomainError::InvalidTelegramChatId)
        );
    }

    #[test]
    fn rejects_lone_hyphen() {
        assert_eq!(
            TelegramChatId::new("-"),
            Err(DomainError::InvalidTelegramChatId)
        );
    }
}
