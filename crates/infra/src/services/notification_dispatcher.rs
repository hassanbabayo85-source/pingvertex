use std::sync::Arc;

use application::ports::{
    MonitorRepository, NotificationChannelRepository, NotificationService, NotifyResult,
    SecretCipher, TenantAlertAllowanceRepository,
};
use application::use_cases::charge_for_usage::ChargeForUsageError;
use application::{MeterUsageInput, MeterUsageUseCase};
use async_trait::async_trait;
use chrono::Utc;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};
use tracing::{info, warn};

use crate::config::SmtpConfig;
use crate::services::{
    DisabledEmailNotificationService, DiscordNotificationService, SlackNotificationService,
    SmsNotificationService, SmtpEmailNotificationService, TelegramNotificationService,
    WebhookNotificationService,
};

pub struct NotificationDispatcher {
    email: Arc<dyn NotificationService>,
    webhook: Arc<dyn NotificationService>,
    slack: Arc<dyn NotificationService>,
    discord: Arc<dyn NotificationService>,
    telegram: Arc<dyn NotificationService>,
    sms: Arc<dyn NotificationService>,
    metering: Option<Arc<MeterUsageUseCase>>,
    allowance_repository: Option<Arc<dyn TenantAlertAllowanceRepository>>,
    monitor_repository: Option<Arc<dyn MonitorRepository>>,
    channel_repository: Option<Arc<dyn NotificationChannelRepository>>,
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
            allowance_repository: None,
            monitor_repository: None,
            channel_repository: None,
        }
    }

    pub fn with_metering(mut self, metering: Arc<MeterUsageUseCase>) -> Self {
        self.metering = Some(metering);
        self
    }

    pub fn with_alert_allowance(
        mut self,
        allowance_repository: Arc<dyn TenantAlertAllowanceRepository>,
        monitor_repository: Arc<dyn MonitorRepository>,
        channel_repository: Arc<dyn NotificationChannelRepository>,
    ) -> Self {
        self.allowance_repository = Some(allowance_repository);
        self.monitor_repository = Some(monitor_repository);
        self.channel_repository = Some(channel_repository);
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
        let slack: Arc<dyn NotificationService> =
            Arc::new(SlackNotificationService::new(cipher.clone()));
        let discord: Arc<dyn NotificationService> =
            Arc::new(DiscordNotificationService::new(cipher.clone()));
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
        let kind = event_kind(event);
        let request_id = if matches!(event, NotificationEvent::Test(_)) {
            format!("test:{}:{}:{}", channel.id.0, kind, uuid::Uuid::new_v4())
        } else {
            format!(
                "notification:{}:{}:{}",
                channel.id.0,
                kind,
                uuid::Uuid::new_v4()
            )
        };
        self.notify_with_request_id(channel, event, &request_id)
            .await
    }

    async fn notify_with_request_id(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
        request_id: &str,
    ) -> NotifyResult {
        let use_allowance = channel.kind == NotificationChannelKind::Sms
            && !request_id.starts_with("test:")
            && self.allowance_repository.is_some()
            && self.monitor_repository.is_some()
            && self.channel_repository.is_some();

        if use_allowance {
            let allowance_repo = self.allowance_repository.as_ref().unwrap();
            let monitor_repo = self.monitor_repository.as_ref().unwrap();
            let channel_repo = self.channel_repository.as_ref().unwrap();
            let tenant_id = channel.tenant_id;

            let enabled = monitor_repo
                .count_enabled_for_tenant(tenant_id)
                .await
                .unwrap_or(0);
            if let Err(err) = allowance_repo
                .current_cycle(tenant_id, enabled, Utc::now())
                .await
            {
                warn!(tenant_id = %tenant_id.0, error = %err, "allowance current_cycle lookup failed; falling through to wallet");
            }

            match allowance_repo.try_consume_sms(tenant_id, Utc::now()).await {
                Ok(true) => {
                    return self.sms.notify(channel, event).await;
                }
                Ok(false) => {
                    info!(tenant_id = %tenant_id.0, "SMS allowance exhausted; attempting paid SMS credits");
                }
                Err(err) => {
                    warn!(tenant_id = %tenant_id.0, error = %err, "allowance try_consume_sms failed; falling through to wallet");
                }
            }

            if let Some(metering) = &self.metering {
                let request_id_owned = request_id.to_owned();
                match metering
                    .execute(
                        MeterUsageInput {
                            tenant_id: channel.tenant_id.0,
                            operation: "sms".to_owned(),
                            units: 1,
                            request_id: request_id_owned,
                            monitor_id: None,
                            execution_id: None,
                            pricing_context: None,
                        },
                        || self.sms.notify(channel, event),
                    )
                    .await
                {
                    Ok(_) => return Ok(()),
                    Err(ChargeForUsageError::InsufficientBalance)
                    | Err(ChargeForUsageError::SpendLimitExceeded) => {
                        info!(tenant_id = %tenant_id.0, "SMS wallet exhausted; falling back to email");
                    }
                    Err(err) => {
                        return Err(Box::new(err) as Box<dyn std::error::Error + Send + Sync>);
                    }
                }
            }

            let emails = match channel_repo.list_enabled_email_by_tenant(tenant_id).await {
                Ok(v) => v,
                Err(err) => {
                    warn!(tenant_id = %tenant_id.0, error = %err, "SMS fallback: failed to load tenant email channels");
                    Vec::new()
                }
            };
            if emails.is_empty() {
                info!(tenant_id = %tenant_id.0, "SMS allowance exhausted and no email channels configured; notification dropped");
                return Ok(());
            }
            for email_ch in &emails {
                if let Err(err) = self.email.notify(email_ch, event).await {
                    warn!(
                        tenant_id = %tenant_id.0,
                        channel_id = %email_ch.id.0,
                        error = %err,
                        "SMS fallback: email delivery failed"
                    );
                }
            }
            return Ok(());
        }

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
                        pricing_context: None,
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
