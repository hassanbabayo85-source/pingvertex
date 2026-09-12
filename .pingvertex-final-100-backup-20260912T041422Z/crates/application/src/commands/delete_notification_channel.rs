use domain::shared::ids::NotificationChannelId;

#[derive(Debug, Clone, Copy)]
pub struct DeleteNotificationChannel {
    pub id: NotificationChannelId,
}
