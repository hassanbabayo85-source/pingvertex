use application::ports::{EmailVerificationMailer, MailerResult};
use async_trait::async_trait;
use domain::Email;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};

use crate::config::SmtpConfig;

/// SMTP-backed verification email delivery.
pub struct SmtpEmailVerificationMailer {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpEmailVerificationMailer {
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

#[async_trait]
impl EmailVerificationMailer for SmtpEmailVerificationMailer {
    async fn send_verification_email(&self, to: &Email, raw_token: &str) -> MailerResult<()> {
        let to_mailbox: Mailbox = to.as_str().parse()?;

        let subject = "Your PingVertex verification code";
        let body = format!(
            "Your PingVertex verification code is:\n\n             {raw_token}\n\n             This 6-digit code expires in 10 minutes and can only be used once.              If you did not create a PingVertex account, you can safely ignore this email."
        );

        let email = Message::builder()
            .from(self.from.clone())
            .to(to_mailbox)
            .subject(subject)
            .body(body)?;

        self.mailer.send(email).await?;
        Ok(())
    }
}

/// Fail-loud fallback used when SMTP is not configured. It never
/// silently drops a verification email.
pub struct DisabledEmailVerificationMailer {
    reason: String,
}

impl DisabledEmailVerificationMailer {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl EmailVerificationMailer for DisabledEmailVerificationMailer {
    async fn send_verification_email(&self, _to: &Email, _raw_token: &str) -> MailerResult<()> {
        Err(format!(
            "email verification emails are not configured: {}",
            self.reason
        )
        .into())
    }
}
