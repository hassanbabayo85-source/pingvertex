use std::sync::Arc;
use std::time::Duration;

use application::ports::{NotificationService, NotifyResult, SecretCipher};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};
use serde::Serialize;
use tracing::warn;

const SLACK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct SlackNotificationService {
    cipher: Arc<dyn SecretCipher>,
}

impl SlackNotificationService {
    pub fn new(cipher: Arc<dyn SecretCipher>) -> Self {
        Self { cipher }
    }
}

#[derive(Debug, Serialize)]
struct SlackPayload {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_emoji: Option<String>,
}

fn payload_for(event: &NotificationEvent) -> SlackPayload {
    let text = match event {
        NotificationEvent::MonitorDown(m) => format!("🔴 {} is DOWN", m.name.as_str()),
        NotificationEvent::MonitorRecovered(m) => format!("✅ {} has recovered", m.name.as_str()),
        NotificationEvent::Test(m) => format!("🔔 Test notification for {}", m.name.as_str()),
    };
    SlackPayload {
        text,
        channel: None,
        username: Some("PingVertex".to_string()),
        icon_emoji: Some(":vertical_traffic_light:".to_string()),
    }
}

#[async_trait]
impl NotificationService for SlackNotificationService {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        if channel.kind != NotificationChannelKind::Slack {
            return Err(format!(
                "SlackNotificationService received non-slack channel (kind: {})",
                channel.kind
            )
            .into());
        }

        let encrypted = channel
            .encrypted_secret
            .as_ref()
            .ok_or("slack channel is missing encrypted webhook url")?;
        let url_bytes = self
            .cipher
            .decrypt_bytes(encrypted)
            .map_err(|err| format!("failed to decrypt slack url: {err}"))?;
        let url = std::str::from_utf8(&url_bytes)
            .map_err(|_| "decrypted slack url is not valid UTF-8")?;

        let payload = payload_for(event);
        let body = serde_json::to_vec(&payload)?;

        let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for attempt in 0..3 {
            if attempt > 0 {
                let backoff_ms = 500 * 2u64.pow(attempt - 1);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
            match send_once(url, &body).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    warn!(attempt = attempt + 1, error = %err, "slack notification attempt failed");
                    last_error = Some(err);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| "slack notification failed".into()))
    }
}

async fn send_once(url: &str, body: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let destination = shared::ssrf::resolve_and_validate_destination(url)
        .await
        .map_err(|_| "SSRF validation blocked the destination")?; // Custom message, scrubs url

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        .no_proxy()
        .timeout(SLACK_TIMEOUT)
        .build()
        .map_err(|e| format!("failed to build client: {}", e.without_url()))?;

    let response = client
        .post(destination.url)
        .header("Content-Type", "application/json")
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| format!("network error: {}", e.without_url()))?;

    if !response.status().is_success() {
        return Err(format!("slack endpoint responded with status {}", response.status()).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_once_rejects_private_ip_without_revealing_url() {
        let secret_url = "https://127.0.0.1:8080/services/SECRET_TOKEN_DO_NOT_LEAK";
        let result = send_once(secret_url, b"{}").await;
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("SSRF validation blocked"));
        assert!(!err_msg.contains("SECRET_TOKEN_DO_NOT_LEAK"));
    }

    #[tokio::test]
    async fn send_once_network_error_scrubs_url() {
        // Will fail cleanly due to network timeout or connection refused, keeping the url out of the message.
        let secret_url = "https://8.8.8.8:1/services/SECRET_TOKEN_DO_NOT_LEAK";
        let result = send_once(secret_url, b"{}").await;
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("network error") || err_msg.contains("failed to build client"));
        assert!(!err_msg.contains("SECRET_TOKEN_DO_NOT_LEAK"));
    }
}
