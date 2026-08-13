use domain::shared::ids::{NotificationChannelId, UserId};

#[derive(Debug, Clone, Copy)]
pub struct DeleteNotificationChannel {
    pub id: NotificationChannelId,
    pub user_id: UserId,
}
