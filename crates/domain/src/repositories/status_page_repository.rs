use async_trait::async_trait;

use crate::entities::StatusPage;
use crate::shared::ids::StatusPageId;

#[async_trait]
pub trait StatusPageRepository: Send + Sync {
    async fn create(&self, status_page: &StatusPage);

    async fn update(&self, status_page: &StatusPage);

    async fn delete(&self, id: StatusPageId);

    async fn find_by_id(&self, id: StatusPageId) -> Option<StatusPage>;

    async fn list(&self) -> Vec<StatusPage>;
}
