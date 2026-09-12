use application::ports::{NotificationService, NotifyResult};
use async_trait::async_trait;
use domain::{entities::NotificationChannel, notification::NotificationEvent};

/// Does nothing. Used as the default `NotificationService` wherever a
/// deployment (or a test) doesn't need real notification delivery — the
/// original scaffolding purpose of this type is unchanged, it's just been
/// updated to match the new single-channel `NotificationService::notify`
/// signature.
#[derive(Debug, Clone, Default)]
pub struct NoopNotificationService;

impl NoopNotificationService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl NotificationService for NoopNotificationService {
    async fn notify(
        &self,
        _channel: &NotificationChannel,
        _event: &NotificationEvent,
    ) -> NotifyResult {
        Ok(())
    }
}
