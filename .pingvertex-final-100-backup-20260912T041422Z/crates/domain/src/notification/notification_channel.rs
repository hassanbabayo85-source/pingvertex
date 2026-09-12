use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::NotificationChannelKind;
use crate::errors::DomainError;
use crate::shared::ids::{MonitorId, NotificationChannelId};
use crate::value_objects::Url;
use crate::value_objects::email::{Email, EmailError};

/// A channel through which a monitor's alerts are delivered
/// (e.g. an email address or a webhook URL).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub id: NotificationChannelId,
    pub monitor_id: MonitorId,
    pub kind: NotificationChannelKind,
    pub target: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NotificationChannel {
    /// Creates a new, validated `NotificationChannel`.
    ///
    /// `target` is validated according to `kind`: an email address for
    /// `NotificationChannelKind::Email`, or an http/https URL for
    /// `NotificationChannelKind::Webhook`.
    pub fn new(
        monitor_id: MonitorId,
        kind: NotificationChannelKind,
        target: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let target = Self::validate_target(kind, target.into())?;
        let now = Utc::now();

        Ok(Self {
            id: NotificationChannelId::new(),
            monitor_id,
            kind,
            target,
            created_at: now,
            updated_at: now,
        })
    }

    /// Validates `target` against `kind` and returns the normalized value.
    pub fn validate_target(
        kind: NotificationChannelKind,
        target: String,
    ) -> Result<String, DomainError> {
        let target = target.trim();

        if target.is_empty() {
            return Err(DomainError::EmptyNotificationTarget);
        }

        match kind {
            NotificationChannelKind::Email => {
                let email = Email::new(target).map_err(|err| match err {
                    EmailError::Empty => DomainError::EmptyNotificationTarget,
                    EmailError::TooLong | EmailError::InvalidFormat => {
                        DomainError::InvalidNotificationEmail
                    }
                })?;
                Ok(email.into_string())
            }
            NotificationChannelKind::Webhook => {
                let url = Url::new(target).map_err(|err| match err {
                    DomainError::EmptyUrl => DomainError::EmptyNotificationTarget,
                    _ => DomainError::InvalidNotificationWebhookUrl,
                })?;
                Ok(url.as_str().to_owned())
            }
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        Self::validate_target(self.kind, self.target.clone())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn creates_valid_email_channel() {
        let channel = NotificationChannel::new(
            MonitorId(Uuid::new_v4()),
            NotificationChannelKind::Email,
            "alerts@example.com",
        )
        .unwrap();

        assert_eq!(channel.target, "alerts@example.com");
        assert_eq!(channel.kind, NotificationChannelKind::Email);
    }

    #[test]
    fn creates_valid_webhook_channel() {
        let channel = NotificationChannel::new(
            MonitorId(Uuid::new_v4()),
            NotificationChannelKind::Webhook,
            "https://example.com/hooks/alert",
        )
        .unwrap();

        assert_eq!(channel.target, "https://example.com/hooks/alert");
        assert_eq!(channel.kind, NotificationChannelKind::Webhook);
    }

    #[test]
    fn rejects_empty_target() {
        let result = NotificationChannel::new(
            MonitorId(Uuid::new_v4()),
            NotificationChannelKind::Email,
            "",
        );

        assert_eq!(result.unwrap_err(), DomainError::EmptyNotificationTarget);
    }

    #[test]
    fn rejects_invalid_email() {
        let result = NotificationChannel::new(
            MonitorId(Uuid::new_v4()),
            NotificationChannelKind::Email,
            "not-an-email",
        );

        assert_eq!(result.unwrap_err(), DomainError::InvalidNotificationEmail);
    }

    #[test]
    fn rejects_invalid_webhook_url() {
        let result = NotificationChannel::new(
            MonitorId(Uuid::new_v4()),
            NotificationChannelKind::Webhook,
            "not-a-url",
        );

        assert_eq!(
            result.unwrap_err(),
            DomainError::InvalidNotificationWebhookUrl
        );
    }

    #[test]
    fn rejects_unsupported_webhook_scheme() {
        let result = NotificationChannel::new(
            MonitorId(Uuid::new_v4()),
            NotificationChannelKind::Webhook,
            "ftp://example.com",
        );

        assert_eq!(
            result.unwrap_err(),
            DomainError::InvalidNotificationWebhookUrl
        );
    }
}
