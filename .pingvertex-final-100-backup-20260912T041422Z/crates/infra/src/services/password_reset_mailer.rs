use application::ports::{PasswordResetMailer, PasswordResetMailerResult};
use async_trait::async_trait;
use domain::Email;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};

use crate::config::SmtpConfig;

/// SMTP-backed password-reset email delivery. Structurally identical to
/// `SmtpEmailVerificationMailer` — same transport construction, same
/// `SmtpConfig` — just a different subject/body and port trait.
pub struct SmtpPasswordResetMailer {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpPasswordResetMailer {
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
impl PasswordResetMailer for SmtpPasswordResetMailer {
    async fn send_password_reset_email(
        &self,
        to: &Email,
        raw_token: &str,
    ) -> PasswordResetMailerResult<()> {
        let to_mailbox: Mailbox = to.as_str().parse()?;

        let subject = "Reset your PingVertex password";
        let body = format!(
            "We received a request to reset your PingVertex account password. Use the \
             following token to set a new password:\n\n\
             {raw_token}\n\n\
             This token expires in 30 minutes. If you did not request a password reset, \
             you can safely ignore this email — your password will not be changed."
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

/// Fail-loud fallback used when SMTP is not configured. Never silently
/// drops a password-reset email — mirrors `DisabledEmailVerificationMailer`.
pub struct DisabledPasswordResetMailer {
    reason: String,
}

impl DisabledPasswordResetMailer {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl PasswordResetMailer for DisabledPasswordResetMailer {
    async fn send_password_reset_email(
        &self,
        _to: &Email,
        _raw_token: &str,
    ) -> PasswordResetMailerResult<()> {
        Err(format!("password reset emails are not configured: {}", self.reason).into())
    }
}
