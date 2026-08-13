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

/// Per-attempt network timeout. Keeps one slow/hanging third-party
/// endpoint from tying up a worker task indefinitely.
const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(8);

/// Total attempts (1 initial + up to 2 retries) for a transient failure
/// (network error, non-2xx response, timeout).
const WEBHOOK_MAX_ATTEMPTS: u32 = 3;

/// Base for exponential backoff between retries: 500ms, then 1000ms.
const WEBHOOK_BACKOFF_BASE_MS: u64 = 500;

/// Sends monitor lifecycle notifications as signed HTTP POST requests to a
/// user-supplied URL — the mechanism that lets someone wire this project up
/// to Slack/Discord/PagerDuty/their own endpoint via an incoming webhook.
///
/// Holds no per-instance state (no config needed — target URL and signing
/// secret live on each `NotificationChannel`), so it's cheap to construct
/// and safe to share across the whole app behind one `Arc`.
#[derive(Debug, Clone, Default)]
pub struct WebhookNotificationService;

impl WebhookNotificationService {
    pub fn new() -> Self {
        Self
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

        let secret = channel
            .signing_secret
            .as_deref()
            .ok_or("webhook channel is missing a signing secret")?;

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
                        url = %channel.target,
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
/// what closes the DNS-rebinding gap: a URL that resolved to a public IP
/// when the channel was created (or on a previous retry) could have its
/// DNS record repointed at an internal address by the time this call
/// fires. Re-checking on every attempt, using the same
/// `resolve_and_validate_destination` the worker's monitor checker uses,
/// means that repointed address is caught here too, not just once.
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
