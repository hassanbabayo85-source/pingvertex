use std::error::Error;

use async_trait::async_trait;
use domain::Email;

pub type MailerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Delivers the raw email-verification token to the user's email address.
///
/// This is deliberately a separate port from `NotificationService`:
/// verification emails are transactional account emails, not
/// monitor-lifecycle notifications, and they do not require a
/// `NotificationChannel`.
#[async_trait]
pub trait EmailVerificationMailer: Send + Sync {
    async fn send_verification_email(&self, to: &Email, raw_token: &str) -> MailerResult<()>;
}

use std::sync::Arc;

#[async_trait]
impl<T> EmailVerificationMailer for Arc<T>
where
    T: EmailVerificationMailer + ?Sized,
{
    async fn send_verification_email(&self, to: &Email, raw_token: &str) -> MailerResult<()> {
        (**self).send_verification_email(to, raw_token).await
    }
}
