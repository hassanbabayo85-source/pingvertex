use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

use domain::{entities::NotificationChannel, notification::NotificationEvent};

pub type NotifyResult = Result<(), Box<dyn Error + Send + Sync>>;

/// Delivers a single `NotificationEvent` to a single `NotificationChannel`.
///
/// This is deliberately a ONE-channel-at-a-time port, not "notify everyone
/// subscribed to this monitor". Deciding WHICH channels get notified for a
/// given event is a business decision (see
/// `crates/worker/src/incident_manager.rs::notify_channels`, and
/// `crates/application/src/use_cases/send_test_notification.rs`), and this
/// project's Clean Architecture convention keeps that kind of decision out
/// of infra — infra (`crates/infra/src/services/*_notification_service.rs`)
/// only knows how to deliver, never whether or to whom.
///
/// Two infra adapters implement this: `SmtpEmailNotificationService` and
/// `WebhookNotificationService`. `infra::NotificationDispatcher` composes
/// both behind a single `Arc<dyn NotificationService>` and routes by
/// `channel.kind`, so callers never need to know which adapter handles
/// which channel.
#[async_trait]
pub trait NotificationService: Send + Sync {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult;
    /// Sends an account-level billing email without pretending it is a monitor event.
    /// Default implementations reject it; the notification dispatcher routes it
    /// to the SMTP adapter when email is configured.
    async fn notify_wallet_email(
        &self,
        _recipient: &str,
        _subject: &str,
        _body: &str,
    ) -> NotifyResult {
        Err("wallet email notifications are not supported by this notification service".into())
    }
}

#[async_trait]
impl<T> NotificationService for Arc<T>
where
    T: NotificationService + Send + Sync + ?Sized,
{
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        (**self).notify(channel, event).await
    }

    async fn notify_wallet_email(&self, recipient: &str, subject: &str, body: &str) -> NotifyResult {
        (**self).notify_wallet_email(recipient, subject, body).await
    }
}
