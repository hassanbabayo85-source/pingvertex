use std::collections::HashMap;
use std::time::{Duration, Instant};

use application::ports::{
    NotificationChannelRepository, NotificationOutboxSender, NotificationService,
};
use async_trait::async_trait;
use domain::{
    NotificationOutboxEntry, entities::Monitor, enums::NotificationOutboxEventKind,
    notification::NotificationEvent, shared::ids::TenantId, shared::policies::TenantOwned,
};
use tokio::sync::Mutex;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Default)]
struct RateLimiterInner {
    window_start: Option<Instant>,
    counts: HashMap<TenantId, u64>,
}

/// Per-tenant, in-memory notification sending rate limiter.
///
/// Tracks one fixed window at a time. When the current window expires,
/// the map is cleared and a fresh window starts. Keys are tenant ids so
/// one tenant cannot drown out notification delivery for another tenant
/// behind the same worker process.
pub struct TenantNotificationRateLimiter {
    inner: Mutex<RateLimiterInner>,
    max_notifications: u64,
    window_duration: Duration,
}

impl TenantNotificationRateLimiter {
    pub fn new(max_notifications: u64, window_duration: Duration) -> Self {
        Self {
            inner: Mutex::new(RateLimiterInner::default()),
            max_notifications,
            window_duration,
        }
    }

    /// Records one notification attempt for `tenant_id`.
    ///
    /// Returns an error if the tenant has already sent
    /// `max_notifications` notifications in the current window. The
    /// caller should treat that error as a permanent failure for the
    /// current attempt and let the outbox worker's normal retry/backoff
    /// logic decide when (or whether) to try again.
    pub async fn check(&self, tenant_id: TenantId) -> Result<()> {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();

        let should_reset = match inner.window_start {
            Some(start) => now.duration_since(start) >= self.window_duration,
            None => true,
        };

        if should_reset {
            inner.window_start = Some(now);
            inner.counts.clear();
        }

        let count = inner.counts.entry(tenant_id).or_insert(0);
        if *count >= self.max_notifications {
            return Err(format!(
                "tenant {} exceeded notification rate limit ({} per {:?})",
                tenant_id.0, self.max_notifications, self.window_duration
            )
            .into());
        }

        *count += 1;
        Ok(())
    }
}

/// Real `NotificationOutboxSender` used by B26.
///
/// Resolves the outbox entry's channel id, checks the per-tenant rate
/// limiter, reconstructs the `NotificationEvent` from the persisted
/// `Monitor` JSON payload, and routes the event through the existing
/// `NotificationDispatcher` (`Arc<dyn NotificationService>`). That
/// dispatcher is the single place that knows which email/webhook/slack/
/// discord/telegram/sms adapter to invoke for a given channel kind.
pub struct RealNotificationOutboxSender {
    notification_service: std::sync::Arc<dyn NotificationService>,
    notification_channel_repository: std::sync::Arc<dyn NotificationChannelRepository>,
    rate_limiter: std::sync::Arc<TenantNotificationRateLimiter>,
}

impl RealNotificationOutboxSender {
    pub fn new(
        notification_service: std::sync::Arc<dyn NotificationService>,
        notification_channel_repository: std::sync::Arc<dyn NotificationChannelRepository>,
        rate_limiter: std::sync::Arc<TenantNotificationRateLimiter>,
    ) -> Self {
        Self {
            notification_service,
            notification_channel_repository,
            rate_limiter,
        }
    }
}

#[async_trait]
impl NotificationOutboxSender for RealNotificationOutboxSender {
    async fn send(&self, entry: &NotificationOutboxEntry) -> Result<()> {
        let channel = self
            .notification_channel_repository
            .find_by_id(entry.channel_id)
            .await?
            .ok_or("notification channel not found")?;

        if !channel.enabled {
            return Err(format!("notification channel {} is disabled", channel.id.0).into());
        }

        self.rate_limiter.check(channel.tenant_id()).await?;

        let monitor: Monitor = serde_json::from_str(&entry.payload)?;

        let event = match entry.event_kind {
            NotificationOutboxEventKind::MonitorDown => NotificationEvent::MonitorDown(monitor),
            NotificationOutboxEventKind::MonitorRecovered => {
                NotificationEvent::MonitorRecovered(monitor)
            }
            NotificationOutboxEventKind::Test => NotificationEvent::Test(monitor),
        };

        self.notification_service.notify(&channel, &event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn tenant() -> TenantId {
        TenantId(Uuid::new_v4())
    }

    #[tokio::test]
    async fn rate_limiter_allows_until_limit_then_blocks_same_window() {
        let limiter = TenantNotificationRateLimiter::new(2, Duration::from_secs(60));
        let tenant_a = tenant();
        let tenant_b = tenant();

        assert!(limiter.check(tenant_a).await.is_ok());
        assert!(limiter.check(tenant_a).await.is_ok());
        assert!(limiter.check(tenant_a).await.is_err());
        // Tenant B is unaffected by A's exhaustion.
        assert!(limiter.check(tenant_b).await.is_ok());
    }

    #[tokio::test]
    async fn rate_limiter_resets_after_window_expiry() {
        let limiter = TenantNotificationRateLimiter::new(1, Duration::from_millis(10));
        let tenant_a = tenant();

        assert!(limiter.check(tenant_a).await.is_ok());
        assert!(limiter.check(tenant_a).await.is_err());

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(limiter.check(tenant_a).await.is_ok());
    }
}
