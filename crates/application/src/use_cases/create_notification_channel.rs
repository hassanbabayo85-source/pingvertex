use std::{error::Error, fmt, sync::Arc};

use domain::{
    entities::{CreateEmailChannel, CreateWebhookChannel, NotificationChannel},
    enums::NotificationChannelKind,
    value_objects::{Email, Url},
};

use crate::{
    commands::CreateNotificationChannel,
    ports::{IdGenerator, MonitorRepository, NotificationChannelRepository},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Error returned by `CreateNotificationChannelUseCase`. Kept distinct from
/// a generic boxed error so the HTTP handler can map each case to the
/// right status code (404 vs 422 vs 500) instead of guessing from a string.
#[derive(Debug)]
pub enum CreateNotificationChannelError {
    /// The monitor doesn't exist, or exists but isn't owned by this user.
    /// Deliberately the same outward result either way — see
    /// `NotificationChannelRepository`'s doc comment on IDOR.
    MonitorNotFound,
    /// `target` failed validation for the given `kind` (bad email address,
    /// bad URL, unsupported scheme, ...). Held as a message rather than a
    /// shared error type because `Email::new` and `Url::new` return two
    /// different, unrelated error enums (`EmailError` / `DomainError`).
    InvalidTarget(String),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateNotificationChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MonitorNotFound => write!(f, "monitor not found or access denied"),
            Self::InvalidTarget(msg) => write!(f, "{msg}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateNotificationChannelError {}

/// Creates a notification channel on a monitor, after verifying the
/// monitor is owned by the requesting user.
///
/// Deliberately does NOT perform an SSRF/DNS check on webhook URLs at
/// creation time. That check requires a real DNS lookup (I/O), which this
/// application-layer use case avoids doing directly, and — more
/// importantly — it would only prove the URL was safe *at creation time*.
/// Because DNS answers can change later (DNS rebinding), the ONLY
/// trustworthy place to enforce SSRF protection is immediately before each
/// actual send, which is exactly what `WebhookNotificationService` does on
/// every attempt (including retries) in `crates/infra`. A create-time
/// check here would be redundant at best and give false confidence at
/// worst.
pub struct CreateNotificationChannelUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    id_generator: Arc<dyn IdGenerator>,
}

impl CreateNotificationChannelUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
        id_generator: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            monitor_repository,
            notification_channel_repository,
            id_generator,
        }
    }

    pub async fn execute(
        &self,
        command: CreateNotificationChannel,
    ) -> Result<NotificationChannel, CreateNotificationChannelError> {
        self.monitor_repository
            .find_by_id_for_user(command.monitor_id, command.user_id)
            .await
            .map_err(CreateNotificationChannelError::Other)?
            .ok_or(CreateNotificationChannelError::MonitorNotFound)?;

        let id = self.id_generator.next_notification_channel_id().await;

        let channel = match command.kind {
            NotificationChannelKind::Email => {
                let email = Email::new(command.target).map_err(|err| {
                    CreateNotificationChannelError::InvalidTarget(err.to_string())
                })?;

                NotificationChannel::create_email(CreateEmailChannel {
                    id,
                    monitor_id: command.monitor_id,
                    user_id: command.user_id,
                    email,
                })
            }
            NotificationChannelKind::Webhook => {
                let url = Url::new(command.target).map_err(|err| {
                    CreateNotificationChannelError::InvalidTarget(err.to_string())
                })?;

                let signing_secret = self.id_generator.next_webhook_signing_secret().await;

                NotificationChannel::create_webhook(CreateWebhookChannel {
                    id,
                    monitor_id: command.monitor_id,
                    user_id: command.user_id,
                    url,
                    signing_secret,
                })
            }
        };

        self.notification_channel_repository
            .create(&channel)
            .await
            .map_err(CreateNotificationChannelError::Other)?;

        Ok(channel)
    }
}
