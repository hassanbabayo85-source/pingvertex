use async_trait::async_trait;

use crate::shared::ids::MonitorId;

use crate::entities::Monitor;

#[async_trait]
pub trait MonitorRepository: Send + Sync {
    async fn create(&self, monitor: &Monitor);

    async fn update(&self, monitor: &Monitor);

    async fn delete(&self, id: MonitorId);

    async fn find_by_id(&self, id: MonitorId) -> Option<Monitor>;

    async fn list(&self) -> Vec<Monitor>;
}
