use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

use domain::{
    entities::Subscriber,
    shared::ids::{MonitorId, SubscriberId},
};

#[async_trait]
pub trait SubscriberRepository: Send + Sync {
    async fn save(&self, subscriber: &Subscriber) -> Result<(), Box<dyn Error + Send + Sync>>;

    async fn delete(&self, id: SubscriberId) -> Result<bool, Box<dyn Error + Send + Sync>>;

    async fn find_by_id(
        &self,
        id: SubscriberId,
    ) -> Result<Option<Subscriber>, Box<dyn Error + Send + Sync>>;

    async fn list_by_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> Result<Vec<Subscriber>, Box<dyn Error + Send + Sync>>;
}

#[async_trait]
impl<T> SubscriberRepository for Arc<T>
where
    T: SubscriberRepository + Send + Sync + ?Sized,
{
    async fn save(&self, subscriber: &Subscriber) -> Result<(), Box<dyn Error + Send + Sync>> {
        (**self).save(subscriber).await
    }

    async fn delete(&self, id: SubscriberId) -> Result<bool, Box<dyn Error + Send + Sync>> {
        (**self).delete(id).await
    }

    async fn find_by_id(
        &self,
        id: SubscriberId,
    ) -> Result<Option<Subscriber>, Box<dyn Error + Send + Sync>> {
        (**self).find_by_id(id).await
    }

    async fn list_by_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> Result<Vec<Subscriber>, Box<dyn Error + Send + Sync>> {
        (**self).list_by_monitor(monitor_id).await
    }
}
