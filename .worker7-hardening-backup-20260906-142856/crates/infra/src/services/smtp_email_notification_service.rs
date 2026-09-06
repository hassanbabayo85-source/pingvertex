use application::ports::{NotificationService, NotifyResult};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};

use crate::config::SmtpConfig;

/// Sends monitor lifecycle notifications by email over SMTP.
///
/// Built once at startup from `SmtpConfig` (see
/// `NotificationDispatcher::from_env`) and reused for every send — `lettre`'s
/// `AsyncSmtpTransport` internally pools connections, so there's no need to
/// reconnect per email.
pub struct SmtpEmailNotificationService {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpEmailNotificationService {
    pub fn new(config: SmtpConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let credentials = Credentials::new(config.username.clone(), config.password.clone());

        let builder = if config.starttls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)?
        } else {
            // Mailpit (da sauran plain-SMTP relays na local/e2e) ba sa
            // magana da TLS kwata-kwata. `relay()` koyaushe yana ƙoƙarin
            // implicit-TLS ne, wanda ke haifar da
            // "received corrupt message of type InvalidContentType"
            // lokacin da aka yi masa TLS handshake da plain-SMTP server.
            // `builder_dangerous` shine zaɓin da lettre ke bayarwa musamman
            // don irin wannan yanayi (babu TLS kwata-kwata).
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
        };

        let builder = builder.port(config.port);
        let mailer = if config.username.is_empty() {
            builder.build()
        } else {
            builder.credentials(credentials).build()
        };

        let from: Mailbox = match &config.from_name {
            Some(name) if !name.is_empty() => {
                format!("{name} <{}>", config.from_address).parse()?
            }
            _ => config.from_address.parse()?,
        };

        Ok(Self { mailer, from })
    }
}

fn email_subject_and_body(event: &NotificationEvent) -> (String, String) {
    match event {
        NotificationEvent::MonitorDown(monitor) => (
            format!("🔴 {} is DOWN", monitor.name.as_str()),
            format!(
                "Your monitor \"{}\" ({}) stopped responding as expected and has been marked DOWN.\n\n\
                 We'll send another email as soon as it recovers.",
                monitor.name.as_str(),
                monitor.url.as_str()
            ),
        ),
        NotificationEvent::MonitorRecovered(monitor) => (
            format!("✅ {} has recovered", monitor.name.as_str()),
            format!(
                "Good news — your monitor \"{}\" ({}) is responding normally again.",
                monitor.name.as_str(),
                monitor.url.as_str()
            ),
        ),
        NotificationEvent::Test(monitor) => (
            format!("🔔 Test notification for {}", monitor.name.as_str()),
            format!(
                "This is a test notification for monitor \"{}\" ({}).\n\n\
                 If you received this, this email channel is configured correctly. \
                 No action is needed.",
                monitor.name.as_str(),
                monitor.url.as_str()
            ),
        ),
    }
}

#[async_trait]
impl NotificationService for SmtpEmailNotificationService {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        if channel.kind != NotificationChannelKind::Email {
            return Err(format!(
                "SmtpEmailNotificationService received a non-email channel (kind: {})",
                channel.kind
            )
            .into());
        }

        let to: Mailbox = channel.target.parse().map_err(|err| {
            format!(
                "invalid recipient email address '{}': {err}",
                channel.target
            )
        })?;

        let (subject, body) = email_subject_and_body(event);

        let email = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(subject)
            .body(body)?;

        self.mailer.send(email).await?;

        Ok(())
    }

    async fn notify_wallet_email(&self, recipient: &str, subject: &str, body: &str) -> NotifyResult {
        let to: Mailbox = recipient.parse().map_err(|err| format!("invalid wallet email recipient '{recipient}': {err}"))?;
        let email = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(subject)
            .body(body.to_owned())?;
        self.mailer.send(email).await?;
        Ok(())
    }
}

/// Stand-in `NotificationService` used when SMTP isn't configured for this
/// deployment (see `SmtpConfig::try_from_env`).
///
/// Deliberately fails LOUDLY the moment an email channel is actually
/// triggered (rather than silently no-op'ing like `NoopNotificationService`),
/// with a message that tells whoever's looking at the logs exactly which
/// environment variables to set. This keeps webhook-only deployments fully
/// functional at startup while still surfacing a clear, actionable error
/// on every email attempt until SMTP is configured.
#[derive(Debug, Clone)]
pub struct DisabledEmailNotificationService {
    reason: String,
}

impl DisabledEmailNotificationService {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl NotificationService for DisabledEmailNotificationService {
    async fn notify(
        &self,
        _channel: &NotificationChannel,
        _event: &NotificationEvent,
    ) -> NotifyResult {
        Err(format!("email notifications are not configured: {}", self.reason).into())
    }
}
