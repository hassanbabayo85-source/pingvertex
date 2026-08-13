use std::sync::Arc;

use application::ports::{NotificationService, NotifyResult};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};
use tracing::warn;

use crate::config::SmtpConfig;
use crate::services::{
    DisabledEmailNotificationService, SmtpEmailNotificationService, WebhookNotificationService,
};

/// Routes a notification to the right adapter based on `channel.kind`.
///
/// This is intentionally the ONLY place in the whole codebase that knows
/// both adapters exist. Callers — `worker::IncidentManager` and the API's
/// "send test notification" handler — just hold a single
/// `Arc<dyn NotificationService>` and call `.notify(channel, event)`; they
/// never need to know or care whether a given channel is email or webhook.
pub struct NotificationDispatcher {
    email: Arc<dyn NotificationService>,
    webhook: Arc<dyn NotificationService>,
}

impl NotificationDispatcher {
    pub fn new(email: Arc<dyn NotificationService>, webhook: Arc<dyn NotificationService>) -> Self {
        Self { email, webhook }
    }

    /// Builds a dispatcher from environment variables.
    ///
    /// Webhook delivery is always available — it needs no global
    /// configuration, since each channel carries its own target URL and
    /// signing secret. Email delivery requires `SMTP_*` environment
    /// variables (see `SmtpConfig::try_from_env`); if they're missing or
    /// invalid, this does NOT fail startup — it logs a warning and falls
    /// back to `DisabledEmailNotificationService`, which returns a clear
    /// error only if/when an email channel is actually triggered. A
    /// self-hosted deployment that only wants webhook notifications
    /// shouldn't be forced to configure an SMTP relay just to boot.
    pub fn from_env() -> Self {
        let email: Arc<dyn NotificationService> = match SmtpConfig::try_from_env() {
            Ok(config) => match SmtpEmailNotificationService::new(config) {
                Ok(service) => Arc::new(service),
                Err(err) => {
                    warn!(
                        error = %err,
                        "failed to initialize SMTP transport; email notifications will fail until this is fixed"
                    );
                    Arc::new(DisabledEmailNotificationService::new(err.to_string()))
                }
            },
            Err(err) => {
                warn!(
                    error = %err,
                    "SMTP is not configured; email notification channels will return an error when triggered. \
                     Set SMTP_HOST, SMTP_PORT, SMTP_USERNAME, SMTP_PASSWORD and SMTP_FROM_ADDRESS to enable email notifications."
                );
                Arc::new(DisabledEmailNotificationService::new(err.to_string()))
            }
        };

        let webhook: Arc<dyn NotificationService> = Arc::new(WebhookNotificationService::new());

        Self::new(email, webhook)
    }
}

#[async_trait]
impl NotificationService for NotificationDispatcher {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        match channel.kind {
            NotificationChannelKind::Email => self.email.notify(channel, event).await,
            NotificationChannelKind::Webhook => self.webhook.notify(channel, event).await,
        }
    }
}
