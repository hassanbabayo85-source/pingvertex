use async_trait::async_trait;
use domain::NotificationOutboxEntry;
use std::{error::Error, sync::Arc};

/// Delivers a single queued outbox notification.
///
/// This is the seam the real channel senders plug into.
/// The worker intentionally supplies only a stub implementation.
#[async_trait]
pub trait NotificationOutboxSender: Send + Sync {
    async fn send(
        &self,
        entry: &NotificationOutboxEntry,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
}

#[async_trait]
impl<T> NotificationOutboxSender for Arc<T>
where
    T: NotificationOutboxSender + Send + Sync + ?Sized,
{
    async fn send(
        &self,
        entry: &NotificationOutboxEntry,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        (**self).send(entry).await
    }
}
