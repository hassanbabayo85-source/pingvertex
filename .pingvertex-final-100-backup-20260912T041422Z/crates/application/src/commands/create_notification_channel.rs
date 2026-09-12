use domain::{enums::NotificationChannelKind, shared::ids::MonitorId};

/// Command to create a notification channel on a monitor.
#[derive(Debug, Clone)]
pub struct CreateNotificationChannel {
    pub monitor_id: MonitorId,
    pub kind: NotificationChannelKind,
    pub target: String,
    /// Required (and only meaningful) when `kind == Telegram`.
    pub chat_id: Option<String>,
}
