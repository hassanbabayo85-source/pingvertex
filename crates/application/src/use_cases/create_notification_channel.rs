use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId,
    entities::{CreateEmailChannel, CreateWebhookChannel, NotificationChannel},
    enums::NotificationChannelKind,
    value_objects::{
        DiscordWebhookUrl, Email, PhoneNumber, SlackWebhookUrl, TelegramBotToken, TelegramChatId,
        Url,
    },
};
use serde_json::json;

use crate::{
    commands::CreateNotificationChannel,
    ports::{
        AuditLogEntryRepository, IdGenerator, MonitorRepository, NotificationChannelRepository,
        SecretCipher,
    },
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
pub enum CreateNotificationChannelError {
    Forbidden,
    MonitorNotFound,
    InvalidTarget(String),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateNotificationChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to create notification channels"
            ),
            Self::MonitorNotFound => write!(f, "monitor not found or access denied"),
            Self::InvalidTarget(msg) => write!(f, "{msg}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateNotificationChannelError {}

pub struct CreateNotificationChannelUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    id_generator: Arc<dyn IdGenerator>,
    secret_cipher: Arc<dyn SecretCipher>,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl CreateNotificationChannelUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
        id_generator: Arc<dyn IdGenerator>,
        secret_cipher: Arc<dyn SecretCipher>,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            monitor_repository,
            notification_channel_repository,
            id_generator,
            secret_cipher,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: CreateNotificationChannel,
        authenticated_user: AuthenticatedUser,
    ) -> Result<NotificationChannel, CreateNotificationChannelError> {
        if !authenticated_user.can(Permission::CreateMonitor) {
            return Err(CreateNotificationChannelError::Forbidden);
        }

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let monitor = self
            .monitor_repository
            .find_by_id(command.monitor_id)
            .await
            .map_err(CreateNotificationChannelError::Other)?
            .ok_or(CreateNotificationChannelError::MonitorNotFound)?;

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Err(CreateNotificationChannelError::MonitorNotFound);
        }

        let id = self.id_generator.next_notification_channel_id().await;

        let channel = match command.kind {
            NotificationChannelKind::Email => {
                let email = Email::new(command.target).map_err(|err| {
                    CreateNotificationChannelError::InvalidTarget(err.to_string())
                })?;

                NotificationChannel::create_email(CreateEmailChannel {
                    id,
                    monitor_id: command.monitor_id,
                    user_id,
                    tenant_id,
                    email,
                })
            }
            NotificationChannelKind::Webhook => {
                let url = Url::new(command.target).map_err(|err| {
                    CreateNotificationChannelError::InvalidTarget(err.to_string())
                })?;

                let signing_secret = self.id_generator.next_webhook_signing_secret().await;

                let mut channel = NotificationChannel::create_webhook(CreateWebhookChannel {
                    id,
                    monitor_id: command.monitor_id,
                    user_id,
                    tenant_id,
                    url,
                    signing_secret: signing_secret.clone(),
                });

                let encrypted = self
                    .secret_cipher
                    .encrypt_bytes(signing_secret.as_bytes())
                    .map_err(|err| CreateNotificationChannelError::Other(Box::new(err)))?;
                channel.encrypted_secret = Some(encrypted);

                channel
            }
            NotificationChannelKind::Slack => {
                let url = SlackWebhookUrl::new(command.target).map_err(|err| {
                    CreateNotificationChannelError::InvalidTarget(err.to_string())
                })?;
                NotificationChannel::create_slack(id, command.monitor_id, user_id, tenant_id, &url)
            }
            NotificationChannelKind::Discord => {
                let url = DiscordWebhookUrl::new(command.target).map_err(|err| {
                    CreateNotificationChannelError::InvalidTarget(err.to_string())
                })?;
                NotificationChannel::create_discord(
                    id,
                    command.monitor_id,
                    user_id,
                    tenant_id,
                    &url,
                )
            }
            NotificationChannelKind::Telegram => {
                let token = TelegramBotToken::new(command.target).map_err(|err| {
                    CreateNotificationChannelError::InvalidTarget(err.to_string())
                })?;
                let chat_id_raw = command.chat_id.clone().ok_or_else(|| {
                    CreateNotificationChannelError::InvalidTarget(
                        "chat_id is required for telegram notification channels".to_string(),
                    )
                })?;
                let chat_id = TelegramChatId::new(chat_id_raw).map_err(|err| {
                    CreateNotificationChannelError::InvalidTarget(err.to_string())
                })?;
                let encrypted = self
                    .secret_cipher
                    .encrypt_bytes(token.as_str().as_bytes())
                    .map_err(|err| CreateNotificationChannelError::Other(Box::new(err)))?;
                let mut channel = NotificationChannel::create_telegram(
                    id,
                    command.monitor_id,
                    user_id,
                    tenant_id,
                    &chat_id,
                );
                channel.encrypted_secret = Some(encrypted);
                channel
            }
            NotificationChannelKind::Sms => {
                let phone = PhoneNumber::new(command.target).map_err(|err| {
                    CreateNotificationChannelError::InvalidTarget(err.to_string())
                })?;
                let encrypted = self
                    .secret_cipher
                    .encrypt_bytes(phone.as_str().as_bytes())
                    .map_err(|err| CreateNotificationChannelError::Other(Box::new(err)))?;
                let mut channel = NotificationChannel::create_sms(
                    id,
                    command.monitor_id,
                    user_id,
                    tenant_id,
                    &phone,
                );
                channel.target = "sms".to_string();
                channel.encrypted_secret = Some(encrypted);
                channel
            }
        };

        let mut to_persist = channel.clone();
        if to_persist.kind == NotificationChannelKind::Webhook {
            to_persist.signing_secret = None;
        }

        self.notification_channel_repository
            .create(&to_persist)
            .await
            .map_err(CreateNotificationChannelError::Other)?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::NotificationChannelCreated,
            target_id: channel.id.0.to_string(),
            metadata: json!({
                "monitor_id": channel.monitor_id.0.to_string(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(CreateNotificationChannelError::Other)?;

        Ok(channel)
    }
}
