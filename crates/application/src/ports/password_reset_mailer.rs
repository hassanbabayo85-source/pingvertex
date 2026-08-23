use std::error::Error;

use async_trait::async_trait;
use domain::Email;

pub type PasswordResetMailerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Delivers the raw password-reset token to the user's email address.
///
/// A separate port from `EmailVerificationMailer`, mirroring why that
/// port is itself separate from `NotificationService`: this is a
/// distinct transactional account email carrying a distinct, more
/// sensitive credential (a reset token can be used to take over the
/// account outright), so it gets its own port even though the delivery
/// mechanism (SMTP) ends up being the same in practice.
#[async_trait]
pub trait PasswordResetMailer: Send + Sync {
    async fn send_password_reset_email(
        &self,
        to: &Email,
        raw_token: &str,
    ) -> PasswordResetMailerResult<()>;
}

use std::sync::Arc;

#[async_trait]
impl<T> PasswordResetMailer for Arc<T>
where
    T: PasswordResetMailer + ?Sized,
{
    async fn send_password_reset_email(
        &self,
        to: &Email,
        raw_token: &str,
    ) -> PasswordResetMailerResult<()> {
        (**self).send_password_reset_email(to, raw_token).await
    }
}
