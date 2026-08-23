use std::time::Duration;

use application::ports::{NotificationService, NotifyResult};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use tracing::warn;

type HmacSha256 = Hmac<Sha256>;

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(8);
const WEBHOOK_MAX_ATTEMPTS: u32 = 3;
const WEBHOOK_BACKOFF_BASE_MS: u64 = 500;

#[derive(Clone)]
pub struct WebhookNotificationService {
    cipher: std::sync::Arc<dyn application::ports::SecretCipher>,
}

impl WebhookNotificationService {
    pub fn new(cipher: std::sync::Arc<dyn application::ports::SecretCipher>) -> Self {
        Self { cipher }
    }

    fn resolve_secret(&self, channel: &NotificationChannel) -> Result<String, String> {
        if let Some(encrypted) = channel.encrypted_secret.as_ref() {
            let bytes = self
                .cipher
                .decrypt_bytes(encrypted)
                .map_err(|err| format!("failed to decrypt webhook signing secret: {err}"))?;
            return String::from_utf8(bytes)
                .map_err(|_| "decrypted webhook signing secret is not valid UTF-8".to_string());
        }

        channel
            .signing_secret
            .clone()
            .ok_or_else(|| "webhook channel is missing a signing secret".to_string())
    }
}

#[derive(Debug, Serialize)]
struct WebhookPayload<'a> {
    event: &'a str,
    monitor_id: String,
    monitor_name: &'a str,
    monitor_url: &'a str,
    message: String,
}

fn payload_for(event: &NotificationEvent) -> WebhookPayload<'_> {
    let monitor = event.monitor();

    let (event_name, message) = match event {
        NotificationEvent::MonitorDown(monitor) => {
            ("monitor.down", format!("{} is down", monitor.name.as_str()))
        }
        NotificationEvent::MonitorRecovered(monitor) => (
            "monitor.recovered",
            format!("{} has recovered", monitor.name.as_str()),
        ),
        NotificationEvent::Test(monitor) => (
            "notification.test",
            format!("Test notification for {}", monitor.name.as_str()),
        ),
    };

    WebhookPayload {
        event: event_name,
        monitor_id: monitor.id.to_string(),
        monitor_name: monitor.name.as_str(),
        monitor_url: monitor.url.as_str(),
        message,
    }
}

#[async_trait]
impl NotificationService for WebhookNotificationService {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        if channel.kind != NotificationChannelKind::Webhook {
            return Err(format!(
                "WebhookNotificationService received a non-webhook channel (kind: {})",
                channel.kind
            )
            .into());
        }

        let secret = self.resolve_secret(channel)?;

        let payload = payload_for(event);
        let body = serde_json::to_vec(&payload)?;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|err| format!("invalid webhook signing secret: {err}"))?;
        mac.update(&body);
        let signature = hex::encode(mac.finalize().into_bytes());

        let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;

        for attempt in 0..WEBHOOK_MAX_ATTEMPTS {
            if attempt > 0 {
                let backoff_ms = WEBHOOK_BACKOFF_BASE_MS * 2u64.pow(attempt - 1);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }

            match send_once(&channel.target, &body, &signature).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    warn!(
                        channel_id = %channel.id.0,
                        attempt = attempt + 1,
                        max_attempts = WEBHOOK_MAX_ATTEMPTS,
                        error = %err,
                        "webhook notification attempt failed"
                    );
                    last_error = Some(err);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| "webhook notification failed for an unknown reason".into()))
    }
}

/// Sends a single signed webhook POST.
///
/// Re-resolves and re-validates the destination via `shared::ssrf` on
/// EVERY call — including every retry — never just once up front. This is
/// what closes the DNS-rebinding gap.
async fn send_once(
    url: &str,
    body: &[u8],
    signature: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let destination = shared::ssrf::resolve_and_validate_destination(url).await?;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        .timeout(WEBHOOK_TIMEOUT)
        .build()?;

    let response = client
        .post(destination.url)
        .header("Content-Type", "application/json")
        .header("X-Webhook-Signature", format!("sha256={signature}"))
        .body(body.to_vec())
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!(
            "webhook endpoint responded with status {}",
            response.status()
        )
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SSRF guard must reject a private-IP webhook URL before any network
    /// call is made. The error message must not include the signing secret.
    #[tokio::test]
    async fn send_once_rejects_private_ip_without_revealing_signature() {
        let secret = "whsec_test_secret_do_not_leak";

        let result = send_once("http://127.0.0.1:8080/status", b"{}", secret).await;
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("SSRF blocked"));
        assert!(!err_msg.contains(secret));
    }
}
