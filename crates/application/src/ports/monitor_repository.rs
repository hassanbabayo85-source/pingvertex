use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::{entities::Monitor, shared::ids::MonitorId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait MonitorRepository: Send + Sync {
    async fn save(&self, monitor: &Monitor) -> RepoResult<()>;

    async fn update(&self, monitor: &Monitor) -> RepoResult<()>;

    async fn delete(&self, id: MonitorId) -> RepoResult<bool>;

    async fn find_by_id(&self, id: MonitorId) -> RepoResult<Option<Monitor>>;

    async fn find_all(&self) -> RepoResult<Vec<Monitor>>;

    async fn find_by_id_for_user(
        &self,
        _id: MonitorId,
        _user_id: domain::shared::ids::UserId,
    ) -> RepoResult<Option<Monitor>> {
        unimplemented!()
    }
    async fn find_all_for_user(
        &self,
        _user_id: domain::shared::ids::UserId,
    ) -> RepoResult<Vec<Monitor>> {
        unimplemented!()
    }
    async fn delete_for_user(
        &self,
        _id: MonitorId,
        _user_id: domain::shared::ids::UserId,
    ) -> RepoResult<bool> {
        unimplemented!()
    }

    /// Returns monitors explicitly marked `is_public = true`, for the
    /// unauthenticated public status page.
    async fn find_public(&self) -> RepoResult<Vec<Monitor>> {
        unimplemented!()
    }

    /// Finds the monitor whose `heartbeat_token` matches `token`, used by
    /// the unauthenticated `GET /heartbeat/{token}` endpoint.
    async fn find_by_heartbeat_token(&self, _token: &str) -> RepoResult<Option<Monitor>> {
        unimplemented!()
    }

    /// Updates `last_heartbeat_at` for the given monitor to `now`.
    async fn update_last_heartbeat_at(
        &self,
        _id: MonitorId,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<()> {
        unimplemented!()
    }
}

#[async_trait]
impl<T> MonitorRepository for Arc<T>
where
    T: MonitorRepository + Send + Sync + ?Sized,
{
    async fn save(&self, monitor: &Monitor) -> RepoResult<()> {
        (**self).save(monitor).await
    }

    async fn update(&self, monitor: &Monitor) -> RepoResult<()> {
        (**self).update(monitor).await
    }

    async fn delete(&self, id: MonitorId) -> RepoResult<bool> {
        (**self).delete(id).await
    }

    async fn find_by_id(&self, id: MonitorId) -> RepoResult<Option<Monitor>> {
        (**self).find_by_id(id).await
    }

    async fn find_all(&self) -> RepoResult<Vec<Monitor>> {
        (**self).find_all().await
    }

    async fn find_by_id_for_user(
        &self,
        id: MonitorId,
        user_id: domain::shared::ids::UserId,
    ) -> RepoResult<Option<Monitor>> {
        (**self).find_by_id_for_user(id, user_id).await
    }
    async fn find_all_for_user(
        &self,
        user_id: domain::shared::ids::UserId,
    ) -> RepoResult<Vec<Monitor>> {
        (**self).find_all_for_user(user_id).await
    }
    async fn delete_for_user(
        &self,
        id: MonitorId,
        user_id: domain::shared::ids::UserId,
    ) -> RepoResult<bool> {
        (**self).delete_for_user(id, user_id).await
    }

    async fn find_public(&self) -> RepoResult<Vec<Monitor>> {
        (**self).find_public().await
    }

    async fn find_by_heartbeat_token(&self, token: &str) -> RepoResult<Option<Monitor>> {
        (**self).find_by_heartbeat_token(token).await
    }

    async fn update_last_heartbeat_at(
        &self,
        id: MonitorId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<()> {
        (**self).update_last_heartbeat_at(id, now).await
    }
}
