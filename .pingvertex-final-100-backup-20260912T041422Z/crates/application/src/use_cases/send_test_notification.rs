use std::{error::Error, fmt, sync::Arc};

use domain::{
    notification::NotificationEvent,
    shared::ids::NotificationChannelId,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
};

use crate::ports::{MonitorRepository, NotificationChannelRepository, NotificationService};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Error returned by `SendTestNotificationUseCase`.
#[derive(Debug)]
pub enum SendTestNotificationError {
    Forbidden,
    /// The channel doesn't exist, or exists but isn't owned by this user.
    ChannelNotFound,
    /// The channel's monitor could not be found. Shouldn't normally
    /// happen (the channel's `monitor_id` came from a foreign key with
    /// `ON DELETE CASCADE`), but handled explicitly rather than panicking.
    MonitorNotFound,
    /// The channel was found and ownership confirmed, but the actual send
    /// (SMTP/webhook) failed. Wraps the underlying `NotificationService`
    /// error so the caller can show it to the user — unlike incident
    /// notifications, a test-send failure SHOULD be surfaced, that's the
    /// entire point of a "send test notification" button.
    DeliveryFailed(Box<dyn Error + Send + Sync>),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for SendTestNotificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to send test notifications"
            ),
            Self::ChannelNotFound => write!(f, "notification channel not found or access denied"),
            Self::MonitorNotFound => write!(f, "monitor for this channel no longer exists"),
            Self::DeliveryFailed(err) => write!(f, "failed to deliver test notification: {err}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for SendTestNotificationError {}

/// Sends a `NotificationEvent::Test` to a single channel, so a user can
/// confirm it's configured correctly before relying on it.
#[derive(Clone)]
pub struct SendTestNotificationUseCase {
    notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    monitor_repository: Arc<dyn MonitorRepository>,
    notification_service: Arc<dyn NotificationService>,
}

impl SendTestNotificationUseCase {
    pub fn new(
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
        monitor_repository: Arc<dyn MonitorRepository>,
        notification_service: Arc<dyn NotificationService>,
    ) -> Self {
        Self {
            notification_channel_repository,
            monitor_repository,
            notification_service,
        }
    }

    pub async fn execute(
        &self,
        id: NotificationChannelId,
        authenticated_user: AuthenticatedUser,
    ) -> Result<(), SendTestNotificationError> {
        if !authenticated_user.can(Permission::UpdateMonitor) {
            return Err(SendTestNotificationError::Forbidden);
        }

        let channel = self
            .notification_channel_repository
            .find_by_id(id)
            .await
            .map_err(SendTestNotificationError::Other)?
            .ok_or(SendTestNotificationError::ChannelNotFound)?;

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            channel.tenant_id(),
        ) {
            return Err(SendTestNotificationError::ChannelNotFound);
        }

        let monitor = self
            .monitor_repository
            .find_by_id(channel.monitor_id)
            .await
            .map_err(SendTestNotificationError::Other)?
            .ok_or(SendTestNotificationError::MonitorNotFound)?;

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Err(SendTestNotificationError::MonitorNotFound);
        }

        let event = NotificationEvent::Test(monitor);

        self.notification_service
            .notify(&channel, &event)
            .await
            .map_err(SendTestNotificationError::DeliveryFailed)
    }
}
