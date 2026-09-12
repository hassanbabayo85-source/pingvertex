use async_trait::async_trait;
use domain::NotificationOutboxEntry;
use std::{error::Error, sync::Arc};

/// Delivers a single queued outbox notification.
///
/// This is the seam B26/B27 will replace with real channel senders.
/// B25 intentionally supplies only a stub implementation in the worker.
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
