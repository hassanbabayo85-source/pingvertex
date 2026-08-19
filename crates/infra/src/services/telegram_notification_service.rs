use std::time::Duration;

use application::ports::{NotificationService, NotifyResult};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};
use serde::Serialize;
use tracing::warn;

const TELEGRAM_TIMEOUT: Duration = Duration::from_secs(5);

pub struct TelegramNotificationService {
    cipher: std::sync::Arc<dyn application::ports::SecretCipher>,
}

impl TelegramNotificationService {
    pub fn new(cipher: std::sync::Arc<dyn application::ports::SecretCipher>) -> Self {
        Self { cipher }
    }
}

#[derive(Debug, Serialize)]
struct TelegramPayload<'a> {
    chat_id: &'a str,
    text: String,
}

#[async_trait]
impl NotificationService for TelegramNotificationService {
    async fn notify(
        &self,
        channel: &NotificationChannel,
        event: &NotificationEvent,
    ) -> NotifyResult {
        if channel.kind != NotificationChannelKind::Telegram {
            return Err(format!(
                "TelegramNotificationService received non-telegram channel (kind: {})",
                channel.kind
            )
            .into());
        }

        if channel.target.trim().is_empty() {
            return Err("telegram channel is missing a chat id".into());
        }
        let chat_id = &channel.target;

        let text = match event {
            NotificationEvent::MonitorDown(m) => format!("🔴 {} is DOWN", m.name.as_str()),
            NotificationEvent::MonitorRecovered(m) => {
                format!("✅ {} has recovered", m.name.as_str())
            }
            NotificationEvent::Test(m) => format!("🔔 Test notification for {}", m.name.as_str()),
        };

        let payload = TelegramPayload { chat_id, text };
        let body = serde_json::to_vec(&payload)?;

        let encrypted_token = channel
            .encrypted_secret
            .as_ref()
            .ok_or("telegram channel is missing encrypted bot token")?;
        let token_bytes = self.cipher.decrypt_bytes(encrypted_token)?;
        let token = std::str::from_utf8(&token_bytes)
            .map_err(|_| "decrypted telegram bot token is not valid UTF-8")?;
        let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt - 1))).await;
            }
            match send_once(&url, &body).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    warn!(attempt = attempt + 1, error = %err, "telegram notification attempt failed");
                    last_error = Some(err);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| "telegram notification failed".into()))
    }
}

async fn send_once(url: &str, body: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let destination = shared::ssrf::resolve_and_validate_destination(url).await?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        .timeout(TELEGRAM_TIMEOUT)
        .build()?;
    let response = client
        .post(destination.url)
        .header("Content-Type", "application/json")
        .body(body.to_vec())
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!(
            "telegram endpoint responded with status {}",
            response.status()
        )
        .into());
    }
    Ok(())
}
