use std::{error::Error, fmt, sync::Arc};

use domain::{
    entities::NotificationChannel,
    shared::ids::{MonitorId, UserId},
};

use crate::ports::{MonitorRepository, NotificationChannelRepository};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Error returned by `ListNotificationChannelsUseCase`. Mirrors
/// `ListIncidentsError` — kept distinct so the handler can tell "monitor
/// not found / not yours" (→ 404) apart from a genuine backend failure
/// (→ 500).
#[derive(Debug)]
pub enum ListNotificationChannelsError {
    NotFound,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for ListNotificationChannelsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "monitor not found or access denied"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for ListNotificationChannelsError {}

/// Lists notification channels for a monitor, verifying the user owns the
/// monitor first.
#[derive(Clone)]
pub struct ListNotificationChannelsUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    notification_channel_repository: Arc<dyn NotificationChannelRepository>,
}

impl ListNotificationChannelsUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    ) -> Self {
        Self {
            monitor_repository,
            notification_channel_repository,
        }
    }

    pub async fn execute(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> Result<Vec<NotificationChannel>, ListNotificationChannelsError> {
        // Belt-and-suspenders: confirm the monitor is owned by this user
        // even though `list_by_monitor_for_user` re-checks the same thing
        // at the SQL level. Two independent checks (application + infra)
        // means a bug in either layer alone still can't leak another
        // user's channels.
        self.monitor_repository
            .find_by_id_for_user(monitor_id, user_id)
            .await
            .map_err(ListNotificationChannelsError::Other)?
            .ok_or(ListNotificationChannelsError::NotFound)?;

        self.notification_channel_repository
            .list_by_monitor_for_user(monitor_id, user_id)
            .await
            .map_err(ListNotificationChannelsError::Other)?
            .ok_or(ListNotificationChannelsError::NotFound)
    }
}
