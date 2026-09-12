use std::time::Duration;

use application::ports::{NotificationService, NotifyResult};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel, enums::NotificationChannelKind, notification::NotificationEvent,
};
use serde::Serialize;
use tracing::warn;

const SLACK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
pub struct SlackNotificationService;

impl SlackNotificationService {
    pub fn new() -> Self {
        Self
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

        let payload = payload_for(event);
        let body = serde_json::to_vec(&payload)?;

        let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for attempt in 0..3 {
            if attempt > 0 {
                let backoff_ms = 500 * 2u64.pow(attempt - 1);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
            match send_once(&channel.target, &body).await {
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
    let destination = shared::ssrf::resolve_and_validate_destination(url).await?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        // SECURITY: see the matching comment in
        // webhook_notification_service.rs — without this, a configured
        // HTTP_PROXY/HTTPS_PROXY/ALL_PROXY env var bypasses the
        // `.resolve()` DNS pinning above.
        .no_proxy()
        .timeout(SLACK_TIMEOUT)
        .build()?;
    let response = client
        .post(destination.url)
        .header("Content-Type", "application/json")
        .body(body.to_vec())
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!("slack endpoint responded with status {}", response.status()).into());
    }
    Ok(())
}
