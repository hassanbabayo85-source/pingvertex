use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::policies::{Ownable, TenantOwned};
use crate::{
    enums::NotificationChannelKind,
    shared::ids::{MonitorId, NotificationChannelId, TenantId, UserId},
    value_objects::{Email, Url},
};

/// Data required to create a new email notification channel.
#[derive(Debug, Clone)]
pub struct CreateEmailChannel {
    pub id: NotificationChannelId,
    pub monitor_id: MonitorId,
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub email: Email,
}

/// Data required to create a new webhook notification channel.
#[derive(Clone)]
pub struct CreateWebhookChannel {
    pub id: NotificationChannelId,
    pub monitor_id: MonitorId,
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub url: Url,
    pub signing_secret: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub id: NotificationChannelId,
    pub monitor_id: MonitorId,
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub kind: NotificationChannelKind,
    pub target: String,
    pub signing_secret: Option<String>,
    pub encrypted_secret: Option<Vec<u8>>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NotificationChannel {
    pub fn create_email(input: CreateEmailChannel) -> Self {
        let now = Utc::now();

        Self {
            id: input.id,
            monitor_id: input.monitor_id,
            user_id: input.user_id,
            tenant_id: input.tenant_id,
            kind: NotificationChannelKind::Email,
            target: input.email.into_string(),
            signing_secret: None,
            encrypted_secret: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn create_webhook(input: CreateWebhookChannel) -> Self {
        let now = Utc::now();

        Self {
            id: input.id,
            monitor_id: input.monitor_id,
            user_id: input.user_id,
            tenant_id: input.tenant_id,
            kind: NotificationChannelKind::Webhook,
            target: input.url.as_str().to_owned(),
            signing_secret: Some(input.signing_secret),
            encrypted_secret: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn create_slack(
        id: NotificationChannelId,
        monitor_id: MonitorId,
        user_id: UserId,
        tenant_id: TenantId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            monitor_id,
            user_id,
            tenant_id,
            kind: NotificationChannelKind::Slack,
            target: "***REDACTED***".to_string(),
            signing_secret: None,
            encrypted_secret: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn create_discord(
        id: NotificationChannelId,
        monitor_id: MonitorId,
        user_id: UserId,
        tenant_id: TenantId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            monitor_id,
            user_id,
            tenant_id,
            kind: NotificationChannelKind::Discord,
            target: "***REDACTED***".to_string(),
            signing_secret: None,
            encrypted_secret: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn create_telegram(
        id: NotificationChannelId,
        monitor_id: MonitorId,
        user_id: UserId,
        tenant_id: TenantId,
        chat_id: &crate::value_objects::TelegramChatId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            monitor_id,
            user_id,
            tenant_id,
            kind: NotificationChannelKind::Telegram,
            target: chat_id.as_str().to_owned(),
            signing_secret: None,
            encrypted_secret: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn create_sms(
        id: NotificationChannelId,
        monitor_id: MonitorId,
        user_id: UserId,
        tenant_id: TenantId,
        phone: &crate::value_objects::PhoneNumber,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            monitor_id,
            user_id,
            tenant_id,
            kind: NotificationChannelKind::Sms,
            target: phone.as_str().to_owned(),
            signing_secret: None,
            encrypted_secret: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.updated_at = Utc::now();
    }

    pub fn enable(&mut self) {
        self.enabled = true;
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ids() -> (NotificationChannelId, MonitorId, UserId, TenantId) {
        (
            NotificationChannelId(Uuid::new_v4()),
            MonitorId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            TenantId(Uuid::new_v4()),
        )
    }

    #[test]
    fn create_email_has_no_signing_secret() {
        let (id, monitor_id, user_id, tenant_id) = ids();

        let channel = NotificationChannel::create_email(CreateEmailChannel {
            id,
            monitor_id,
            user_id,
            tenant_id,
            email: Email::new("owner@example.com").unwrap(),
        });

        assert_eq!(channel.kind, NotificationChannelKind::Email);
        assert_eq!(channel.target, "owner@example.com");
        assert_eq!(channel.signing_secret, None);
        assert!(channel.enabled);
    }

    #[test]
    fn create_webhook_carries_signing_secret() {
        let (id, monitor_id, user_id, tenant_id) = ids();

        let channel = NotificationChannel::create_webhook(CreateWebhookChannel {
            id,
            monitor_id,
            user_id,
            tenant_id,
            url: Url::new("https://hooks.example.com/abc").unwrap(),
            signing_secret: "whsec_test".to_string(),
        });

        assert_eq!(channel.kind, NotificationChannelKind::Webhook);
        assert_eq!(channel.signing_secret.as_deref(), Some("whsec_test"));
    }

    #[test]
    fn create_slack_redacts_target() {
        let (id, monitor_id, user_id, tenant_id) = ids();
        let channel = NotificationChannel::create_slack(id, monitor_id, user_id, tenant_id);

        assert_eq!(channel.kind, NotificationChannelKind::Slack);
        assert_eq!(channel.target, "***REDACTED***");
        assert_eq!(channel.encrypted_secret, None);
    }

    #[test]
    fn create_discord_redacts_target() {
        let (id, monitor_id, user_id, tenant_id) = ids();
        let channel = NotificationChannel::create_discord(id, monitor_id, user_id, tenant_id);

        assert_eq!(channel.kind, NotificationChannelKind::Discord);
        assert_eq!(channel.target, "***REDACTED***");
        assert_eq!(channel.encrypted_secret, None);
    }

    #[test]
    fn disable_then_enable_flips_flag() {
        let (id, monitor_id, user_id, tenant_id) = ids();

        let mut channel = NotificationChannel::create_email(CreateEmailChannel {
            id,
            monitor_id,
            user_id,
            tenant_id,
            email: Email::new("owner@example.com").unwrap(),
        });

        channel.disable();
        assert!(!channel.enabled);

        channel.enable();
        assert!(channel.enabled);
    }
}

impl TenantOwned for NotificationChannel {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

impl Ownable for NotificationChannel {
    fn owner_id(&self) -> UserId {
        self.user_id
    }
}

impl std::fmt::Debug for NotificationChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationChannel")
            .field("id", &self.id)
            .field("tenant_id", &self.tenant_id)
            .field("monitor_id", &self.monitor_id)
            .field("user_id", &self.user_id)
            .field("kind", &self.kind)
            .field("enabled", &self.enabled)
            .field("target", &"[REDACTED]")
            .field(
                "signing_secret",
                &self.signing_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "encrypted_secret",
                &self.encrypted_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

impl std::fmt::Debug for CreateWebhookChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateWebhookChannel")
            .field("id", &self.id)
            .field("monitor_id", &self.monitor_id)
            .field("user_id", &self.user_id)
            .field("tenant_id", &self.tenant_id)
            .field("url", &self.url)
            .field("signing_secret", &"[REDACTED]")
            .finish()
    }
}
