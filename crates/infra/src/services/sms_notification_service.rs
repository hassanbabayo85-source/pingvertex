use std::time::Duration;

use application::ports::{NotificationService, NotifyResult, SecretCipher};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};
use tracing::warn;

use crate::twilio_config::TwilioConfig;

const SMS_TIMEOUT: Duration = Duration::from_secs(5);

/// SMS notification service (Twilio).
///
/// Holds a `SecretCipher` so that the encrypted `encrypted_secret` field
/// (the destination phone number, encrypted by
/// `CreateNotificationChannelUseCase`) can be decrypted before use.
/// `channel.target` is deliberately NOT the phone number for SMS channels
/// (`CreateNotificationChannelUseCase` sets it to the literal string
/// `"sms"` — see that use case) so the number never appears in plaintext
/// anywhere the target column is logged or displayed.
///
/// Mirrors `TelegramNotificationService`'s pattern: SSRF-safe outbound
/// request via `shared::ssrf::resolve_and_validate_destination`, 3 retries
/// with exponential backoff, no credentials in any returned error string.
pub struct SmsNotificationService {
    cipher: std::sync::Arc<dyn SecretCipher>,
}

impl SmsNotificationService {
    pub fn new(cipher: std::sync::Arc<dyn SecretCipher>) -> Self {
        Self { cipher }
    }
}

fn message_text(event: &NotificationEvent) -> String {
    match event {
        NotificationEvent::MonitorDown(m) => format!("PingVertex: {} is DOWN", m.name.as_str()),
        NotificationEvent::MonitorRecovered(m) => {
            format!("PingVertex: {} has recovered", m.name.as_str())
        }
        NotificationEvent::Test(m) => {
            format!("PingVertex: test notification for {}", m.name.as_str())
        }
    }
}

#[async_trait]
impl NotificationService for SmsNotificationService {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        if channel.kind != NotificationChannelKind::Sms {
            return Err(format!(
                "SmsNotificationService received non-sms channel (kind: {})",
                channel.kind
            )
            .into());
        }

        let encrypted = channel
            .encrypted_secret
            .as_ref()
            .ok_or("sms channel is missing encrypted phone number")?;
        let phone_bytes = self
            .cipher
            .decrypt_bytes(encrypted)
            .map_err(|err| format!("failed to decrypt sms phone number: {err}"))?;
        let to_number = String::from_utf8(phone_bytes)
            .map_err(|_| "decrypted sms phone number is not valid UTF-8")?;

        let config = TwilioConfig::try_from_env().map_err(|err| {
            format!(
                "SMS provider not configured: {err}. Set TWILIO_ACCOUNT_SID, TWILIO_AUTH_TOKEN, TWILIO_FROM_NUMBER to enable SMS notifications"
            )
        })?;

        let body = message_text(event);
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            config.account_sid
        );

        let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt - 1))).await;
            }
            match send_once(&url, &config, &to_number, &body).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    // Never let the underlying error carry the auth token or
                    // phone number — send_once's own error strings are
                    // status-code/transport-only (see below), so this is
                    // safe to log and to surface to callers.
                    warn!(attempt = attempt + 1, error = %err, "sms notification attempt failed");
                    last_error = Some(err);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| "sms notification failed".into()))
    }
}

async fn send_once(
    url: &str,
    config: &TwilioConfig,
    to_number: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let destination = shared::ssrf::resolve_and_validate_destination(url).await?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        .timeout(SMS_TIMEOUT)
        .build()?;

    let response = client
        .post(destination.url)
        .basic_auth(&config.account_sid, Some(&config.auth_token))
        .form(&[
            ("To", to_number),
            ("From", config.from_number.as_str()),
            ("Body", body),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        // Deliberately status-code only: Twilio error bodies can echo back
        // request parameters, and we never want the destination number or
        // any credential fragment ending up in a propagated error string.
        return Err(format!(
            "twilio endpoint responded with status {}",
            response.status()
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::SecretCipherError;
    use domain::{
        entities::{CreateEmailChannel, NotificationChannel},
        shared::ids::{MonitorId, NotificationChannelId, TenantId, UserId},
        value_objects::Email,
    };
    use uuid::Uuid;

    /// No-op cipher: returns the plaintext unchanged. Sufficient for these
    /// unit tests, which only exercise the pre-network validation branches
    /// (kind check, missing-secret check, decrypt-failure propagation) —
    /// the same scope `TelegramNotificationService`/`DiscordNotificationService`/
    /// `SlackNotificationService` have (none of them unit-test the live HTTP
    /// call either; see B27 HANDOVER.md "Deviations from prompt").
    struct PassthroughCipher;

    impl SecretCipher for PassthroughCipher {
        fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
            Ok(plaintext.to_vec())
        }
        fn decrypt_bytes(&self, ciphertext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
            Ok(ciphertext.to_vec())
        }
    }

    struct FailingCipher;

    impl SecretCipher for FailingCipher {
        fn encrypt_bytes(&self, _plaintext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
            Err(SecretCipherError::EncryptionFailed("boom".into()))
        }
        fn decrypt_bytes(&self, _ciphertext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
            Err(SecretCipherError::DecryptionFailed("boom".into()))
        }
    }

    fn ids() -> (NotificationChannelId, MonitorId, UserId, TenantId) {
        (
            NotificationChannelId(Uuid::new_v4()),
            MonitorId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            TenantId(Uuid::new_v4()),
        )
    }

    fn email_channel() -> NotificationChannel {
        let (id, monitor_id, user_id, tenant_id) = ids();
        NotificationChannel::create_email(CreateEmailChannel {
            id,
            monitor_id,
            user_id,
            tenant_id,
            email: Email::new("owner@example.com").unwrap(),
        })
    }

    fn sms_channel(encrypted_secret: Option<Vec<u8>>) -> NotificationChannel {
        let (id, monitor_id, user_id, tenant_id) = ids();
        let phone = domain::value_objects::PhoneNumber::new("+15555550100").unwrap();
        let mut channel =
            NotificationChannel::create_sms(id, monitor_id, user_id, tenant_id, &phone);
        channel.target = "sms".to_string();
        channel.encrypted_secret = encrypted_secret;
        channel
    }

    fn test_event() -> NotificationEvent {
        use domain::{
            CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName, Timeout,
        };
        let monitor = MonitorBuilder::new(
            MonitorId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            TenantId::new(),
        )
        .name(MonitorName::new("Test Monitor").unwrap())
        .target("http://example.com".to_string())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .build();
        NotificationEvent::Test(monitor)
    }

    #[tokio::test]
    async fn rejects_non_sms_channel() {
        let service = SmsNotificationService::new(std::sync::Arc::new(PassthroughCipher));
        let result = service.notify(&email_channel(), &test_event()).await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-sms"));
    }

    #[tokio::test]
    async fn errors_when_encrypted_secret_missing() {
        let service = SmsNotificationService::new(std::sync::Arc::new(PassthroughCipher));
        let result = service.notify(&sms_channel(None), &test_event()).await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing encrypted phone number"));
    }

    #[tokio::test]
    async fn errors_when_decryption_fails_and_does_not_leak_ciphertext() {
        let service = SmsNotificationService::new(std::sync::Arc::new(FailingCipher));
        let channel = sms_channel(Some(vec![1, 2, 3, 4]));
        let result = service.notify(&channel, &test_event()).await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed to decrypt sms phone number"));
        // Redaction check: raw ciphertext bytes never surface in the error.
        assert!(!err.contains('\u{1}'));
    }

    #[tokio::test]
    async fn errors_with_provider_not_configured_when_twilio_env_missing_and_never_leaks_phone() {
        // Ensures a real (decryptable) phone number never appears in the
        // returned error when Twilio just isn't configured — the number
        // was successfully decrypted here (unlike the tests above), so
        // this specifically checks the config-missing branch redacts it.
        let service = SmsNotificationService::new(std::sync::Arc::new(PassthroughCipher));
        let channel = sms_channel(Some(b"+15555550100".to_vec()));
        let result = service.notify(&channel, &test_event()).await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("SMS provider not configured"));
        assert!(!err.contains("+15555550100"));
    }
}
