use std::sync::Arc;

use application::ports::{NotificationService, NotifyResult, SecretCipher};
use application::{MeterUsageInput, MeterUsageUseCase};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};
use tracing::warn;

use crate::config::SmtpConfig;
use crate::services::{
    DisabledEmailNotificationService, DiscordNotificationService, SlackNotificationService,
    SmsNotificationService, SmtpEmailNotificationService, TelegramNotificationService,
    WebhookNotificationService,
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
    slack: Arc<dyn NotificationService>,
    discord: Arc<dyn NotificationService>,
    telegram: Arc<dyn NotificationService>,
    sms: Arc<dyn NotificationService>,
    metering: Option<Arc<MeterUsageUseCase>>,
}

impl NotificationDispatcher {
    pub fn new(
        email: Arc<dyn NotificationService>,
        webhook: Arc<dyn NotificationService>,
        slack: Arc<dyn NotificationService>,
        discord: Arc<dyn NotificationService>,
        telegram: Arc<dyn NotificationService>,
        sms: Arc<dyn NotificationService>,
    ) -> Self {
        Self {
            email,
            webhook,
            slack,
            discord,
            telegram,
            sms,
            metering: None,
        }
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
    /// Enables PAYG billing for SMS delivery. Billing lives at this routing
    /// boundary so every SMS path (incident, outbox retry, and test send) is
    /// covered without putting wallet concerns into the Twilio adapter.
    pub fn with_metering(mut self, metering: Arc<MeterUsageUseCase>) -> Self {
        self.metering = Some(metering);
        self
    }

    pub fn from_env_with_metering(
        cipher: Arc<dyn SecretCipher>,
        metering: Arc<MeterUsageUseCase>,
    ) -> Self {
        Self::from_env(cipher).with_metering(metering)
    }

    pub fn from_env(cipher: Arc<dyn SecretCipher>) -> Self {
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

        let webhook: Arc<dyn NotificationService> =
            Arc::new(WebhookNotificationService::new(cipher.clone()));
        let slack: Arc<dyn NotificationService> = Arc::new(SlackNotificationService::new());
        let discord: Arc<dyn NotificationService> = Arc::new(DiscordNotificationService::new());
        let telegram: Arc<dyn NotificationService> =
            Arc::new(TelegramNotificationService::new(cipher.clone()));
        let sms: Arc<dyn NotificationService> = Arc::new(SmsNotificationService::new(cipher));

        Self::new(email, webhook, slack, discord, telegram, sms)
    }
}

fn event_kind(event: &NotificationEvent) -> &'static str {
    match event {
        NotificationEvent::MonitorDown(_) => "monitor_down",
        NotificationEvent::MonitorRecovered(_) => "monitor_recovered",
        NotificationEvent::Test(_) => "test",
    }
}

#[async_trait]
impl NotificationService for NotificationDispatcher {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        let request_id = format!(
            "notification:{}:{}:{}",
            channel.id.0,
            event_kind(event),
            uuid::Uuid::new_v4()
        );
        self.notify_with_request_id(channel, event, &request_id)
            .await
    }

    async fn notify_with_request_id(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
        request_id: &str,
    ) -> NotifyResult {
        if channel.kind == NotificationChannelKind::Sms
            && let Some(metering) = &self.metering
        {
            let request_id = request_id.to_owned();
            return metering
                .execute(
                    MeterUsageInput {
                        tenant_id: channel.tenant_id.0,
                        operation: "sms".to_owned(),
                        units: 1,
                        request_id,

                        monitor_id: None,
                        execution_id: None,
                    },
                    || self.sms.notify(channel, event),
                )
                .await
                .map(|_| ())
                .map_err(|err| Box::new(err) as Box<dyn std::error::Error + Send + Sync>);
        }

        match channel.kind {
            NotificationChannelKind::Email => self.email.notify(channel, event).await,
            NotificationChannelKind::Webhook => self.webhook.notify(channel, event).await,
            NotificationChannelKind::Slack => self.slack.notify(channel, event).await,
            NotificationChannelKind::Discord => self.discord.notify(channel, event).await,
            NotificationChannelKind::Telegram => self.telegram.notify(channel, event).await,
            NotificationChannelKind::Sms => self.sms.notify(channel, event).await,
        }
    }

    async fn notify_wallet_email(
        &self,
        recipient: &str,
        subject: &str,
        body: &str,
    ) -> NotifyResult {
        self.email
            .notify_wallet_email(recipient, subject, body)
            .await
    }
}
