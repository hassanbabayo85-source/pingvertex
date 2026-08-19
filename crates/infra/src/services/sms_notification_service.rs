use application::ports::{NotificationService, NotifyResult, SecretCipher};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};

/// SMS notification service (Twilio/Africa's Talking).
///
/// Holds a `SecretCipher` so that the encrypted `encrypted_secret` field
/// (a phone number, encrypted by `CreateNotificationChannelUseCase`) can
/// be decrypted before use. If the provider credentials are missing, a
/// clear error is returned *after* decryption — this ensures the cipher is
/// actually used and validates the encrypted data path.
pub struct SmsNotificationService {
    cipher: std::sync::Arc<dyn SecretCipher>,
}

impl SmsNotificationService {
    pub fn new(cipher: std::sync::Arc<dyn SecretCipher>) -> Self {
        Self { cipher }
    }
}

#[async_trait]
impl NotificationService for SmsNotificationService {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        _event: &NotificationEvent,
    ) -> NotifyResult {
        if channel.kind != NotificationChannelKind::Sms {
            return Err(format!(
                "SmsNotificationService received non-sms channel (kind: {})",
                channel.kind
            )
            .into());
        }

        // Decrypt phone number from encrypted_secret. This is what makes the
        // cipher actually be read/used — and what the warning about `cipher`
        // was pointing at.
        let encrypted = channel
            .encrypted_secret
            .as_ref()
            .ok_or("sms channel is missing encrypted phone number")?;
        let phone_bytes = self
            .cipher
            .decrypt_bytes(encrypted)
            .map_err(|err| format!("failed to decrypt sms phone number: {err}"))?;
        let _phone_number = String::from_utf8(phone_bytes)
            .map_err(|_| "decrypted sms phone number is not valid UTF-8")?;

        // Check provider configuration.
        let twilio_sid = std::env::var("TWILIO_ACCOUNT_SID").ok();
        let twilio_token = std::env::var("TWILIO_AUTH_TOKEN").ok();
        let from_number = std::env::var("TWILIO_FROM_NUMBER").ok();

        if twilio_sid.is_none() || twilio_token.is_none() || from_number.is_none() {
            return Err(
                "SMS provider not configured; set TWILIO_ACCOUNT_SID, TWILIO_AUTH_TOKEN, TWILIO_FROM_NUMBER to enable SMS notifications".into()
            );
        }

        // Provider credentials are present but the actual Twilio/Africa's Talking
        // HTTP integration is not implemented yet. Fail loudly rather than pretend.
        Err("SMS provider integration not yet implemented (Twilio credentials present)".into())
    }
}
