use domain::{enums::NotificationChannelKind, shared::ids::MonitorId, shared::ids::UserId};

/// Command to create a notification channel on a monitor.
///
/// `target` is the raw, not-yet-validated user input: an email address
/// when `kind == Email`, a URL when `kind == Webhook`. Validation happens
/// inside `CreateNotificationChannelUseCase` via the `Email`/`Url` domain
/// value objects, matching how `CreateMonitorRequest` passes raw strings
/// that `CreateMonitorUseCase` validates.
#[derive(Debug, Clone)]
pub struct CreateNotificationChannel {
    pub monitor_id: MonitorId,
    pub user_id: UserId,
    pub kind: NotificationChannelKind,
    pub target: String,
}
