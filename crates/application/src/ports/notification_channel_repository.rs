use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

use domain::{
    entities::NotificationChannel,
    shared::ids::{MonitorId, NotificationChannelId, UserId},
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persists and queries `NotificationChannel`s.
///
/// Every method that can be reached from an authenticated HTTP request
/// takes a `user_id` and enforces ownership itself (at the SQL level in the
/// Postgres implementation — `WHERE ... AND user_id = $N`, joined through
/// the owning monitor on writes) rather than leaving that check to callers.
/// This mirrors `MonitorRepository::find_by_id_for_user` /
/// `delete_for_user` elsewhere in this codebase, and exists specifically to
/// prevent IDOR: a handler that forgets to check ownership still can't leak
/// or mutate another user's channel, because the repository itself refuses.
///
/// `list_enabled_by_monitor` is the one exception — it takes no `user_id`
/// because it is only ever called from the worker (`IncidentManager`),
/// which has no authenticated user in its context and already knows the
/// monitor is legitimate (it just finished checking it).
#[async_trait]
pub trait NotificationChannelRepository: Send + Sync {
    async fn create(&self, channel: &NotificationChannel) -> RepoResult<()>;

    /// Deletes a channel. Returns `Ok(false)` (NOT an error) if no channel
    /// with that id exists, or if it exists but does not belong to
    /// `user_id` — the two cases are indistinguishable to the caller by
    /// design, so ownership can never be probed via error-vs-not-found
    /// timing/response differences.
    async fn delete_for_user(&self, id: NotificationChannelId, user_id: UserId)
    -> RepoResult<bool>;

    /// Fetches a single channel, but only if it belongs to `user_id`.
    async fn find_by_id_for_user(
        &self,
        id: NotificationChannelId,
        user_id: UserId,
    ) -> RepoResult<Option<NotificationChannel>>;

    /// Lists every channel configured for `monitor_id`, but only if
    /// `monitor_id` itself belongs to `user_id`.
    ///
    /// Returns `Ok(None)` (rather than `Ok(vec![])`) when the monitor
    /// doesn't exist or isn't owned by `user_id`, so the handler can return
    /// 404 instead of an empty 200 — an empty 200 would leak "this monitor
    /// ID exists, it's just not yours" to an attacker probing IDs.
    async fn list_by_monitor_for_user(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<Vec<NotificationChannel>>>;

    /// Lists every ENABLED channel for a monitor, with no ownership check.
    /// Worker-only — see the trait doc comment above.
    async fn list_enabled_by_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> RepoResult<Vec<NotificationChannel>>;

    /// Fetches a single channel with NO ownership filtering. For callers
    /// that authorize the result themselves against the caller's tenant
    /// (e.g. via `AuthorizationPolicy::same_tenant`) instead of the
    /// single-owner `find_by_id_for_user` check.
    async fn find_by_id(&self, _id: NotificationChannelId) -> RepoResult<Option<NotificationChannel>> {
        unimplemented!()
    }

    /// Deletes a channel by id with NO ownership filtering. Callers must
    /// authorize the deletion themselves before calling this (see
    /// `find_by_id` above).
    async fn delete(&self, _id: NotificationChannelId) -> RepoResult<bool> {
        unimplemented!()
    }

    /// Lists every channel configured for `monitor_id`, with no
    /// ownership check. Safe to use once the caller has already verified
    /// `monitor_id` belongs to their tenant (e.g. via
    /// `MonitorRepository::find_by_id` + `AuthorizationPolicy::same_tenant`),
    /// which makes a further per-row ownership check redundant. Unlike
    /// `list_by_monitor_for_user`, this returns a plain `Vec` (not
    /// `Option<Vec<_>>`) — the monitor-existence check happens before
    /// this is called.
    #[allow(unused_variables)]
    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<NotificationChannel>> {
        unimplemented!()
    }
}

#[async_trait]
impl<T> NotificationChannelRepository for Arc<T>
where
    T: NotificationChannelRepository + Send + Sync + ?Sized,
{
    async fn create(&self, channel: &NotificationChannel) -> RepoResult<()> {
        (**self).create(channel).await
    }

    async fn delete_for_user(
        &self,
        id: NotificationChannelId,
        user_id: UserId,
    ) -> RepoResult<bool> {
        (**self).delete_for_user(id, user_id).await
    }

    async fn find_by_id_for_user(
        &self,
        id: NotificationChannelId,
        user_id: UserId,
    ) -> RepoResult<Option<NotificationChannel>> {
        (**self).find_by_id_for_user(id, user_id).await
    }

    async fn list_by_monitor_for_user(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<Vec<NotificationChannel>>> {
        (**self).list_by_monitor_for_user(monitor_id, user_id).await
    }

    async fn list_enabled_by_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> RepoResult<Vec<NotificationChannel>> {
        (**self).list_enabled_by_monitor(monitor_id).await
    }

    async fn find_by_id(&self, id: NotificationChannelId) -> RepoResult<Option<NotificationChannel>> {
        (**self).find_by_id(id).await
    }

    async fn delete(&self, id: NotificationChannelId) -> RepoResult<bool> {
        (**self).delete(id).await
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<NotificationChannel>> {
        (**self).list_by_monitor(monitor_id).await
    }
}
