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
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)?
        };

        let mailer = builder.port(config.port).credentials(credentials).build();

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

        let subject = "Verify your PingVertex email address";
        let body = format!(
            "Please verify your PingVertex account email address by using the following token:\n\n\
             {raw_token}\n\n\
             This token expires in 24 hours. If you did not create an account, you can safely ignore this email."
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
