use crate::errors::DomainError;

/// A validated Telegram bot token.
///
/// Format per BotFather: `digits:alphanumeric_-` (e.g. `110201543:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramBotToken(String);

impl TelegramBotToken {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();

        if value.is_empty() {
            return Err(DomainError::InvalidTelegramBotToken);
        }

        let mut parts = value.split(':');
        let id = parts.next().unwrap_or("");
        let secret = parts.next().unwrap_or("");

        if parts.next().is_some() || id.is_empty() || secret.is_empty() {
            return Err(DomainError::InvalidTelegramBotToken);
        }

        if !id.chars().all(|c| c.is_ascii_digit()) {
            return Err(DomainError::InvalidTelegramBotToken);
        }

        if !secret
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(DomainError::InvalidTelegramBotToken);
        }

        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TelegramBotToken {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_bot_token() {
        assert!(TelegramBotToken::new("123456:ABC-def_123").is_ok());
    }

    #[test]
    fn rejects_missing_secret() {
        assert_eq!(
            TelegramBotToken::new("123456"),
            Err(DomainError::InvalidTelegramBotToken)
        );
    }

    #[test]
    fn rejects_invalid_chars_in_secret() {
        assert_eq!(
            TelegramBotToken::new("123456:ABC def"),
            Err(DomainError::InvalidTelegramBotToken)
        );
    }
}
