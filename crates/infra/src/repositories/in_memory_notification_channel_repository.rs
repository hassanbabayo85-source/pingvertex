use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, RwLock},
};

use application::ports::{MonitorRepository, NotificationChannelRepository};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel,
    enums::NotificationChannelKind,
    shared::ids::{MonitorId, NotificationChannelId, TenantId, UserId},
};

use crate::repositories::InMemoryMonitorRepository;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Simple in-memory notification channel repository for development and
/// tests — mirrors `InMemoryMonitorRepository`.
///
/// Holds a reference to an `InMemoryMonitorRepository` (rather than being
/// fully standalone) specifically so `list_by_monitor_for_user` can
/// faithfully reproduce the real (`Postgres`) repository's "monitor
/// doesn't exist / isn't yours → `None`" vs "owned monitor with zero
/// channels → `Some(vec![])`" distinction, without duplicating a second
/// monitor store. Tests that exercise this repository should construct it
/// with the SAME `InMemoryMonitorRepository` instance used elsewhere in
/// the test, so monitors created through one show up as ownable through
/// the other.
#[derive(Debug, Clone)]
pub struct InMemoryNotificationChannelRepository {
    channels: Arc<RwLock<HashMap<NotificationChannelId, NotificationChannel>>>,
    monitors: InMemoryMonitorRepository,
}

impl InMemoryNotificationChannelRepository {
    pub fn new(monitors: InMemoryMonitorRepository) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            monitors,
        }
    }
}

impl Default for InMemoryNotificationChannelRepository {
    fn default() -> Self {
        Self::new(InMemoryMonitorRepository::new())
    }
}

#[async_trait]
impl NotificationChannelRepository for InMemoryNotificationChannelRepository {
    async fn create(&self, channel: &NotificationChannel) -> RepoResult<()> {
        self.channels
            .write()
            .unwrap()
            .insert(channel.id, channel.clone());

        Ok(())
    }

    async fn delete_for_user(
        &self,
        id: NotificationChannelId,
        user_id: UserId,
    ) -> RepoResult<bool> {
        let mut channels = self.channels.write().unwrap();

        let belongs_to_user = channels
            .get(&id)
            .map(|channel| channel.user_id == user_id)
            .unwrap_or(false);

        if belongs_to_user {
            Ok(channels.remove(&id).is_some())
        } else {
            Ok(false)
        }
    }

    async fn find_by_id_for_user(
        &self,
        id: NotificationChannelId,
        user_id: UserId,
    ) -> RepoResult<Option<NotificationChannel>> {
        Ok(self
            .channels
            .read()
            .unwrap()
            .get(&id)
            .filter(|channel| channel.user_id == user_id)
            .cloned())
    }

    async fn list_by_monitor_for_user(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<Vec<NotificationChannel>>> {
        let owns_monitor = self
            .monitors
            .find_by_id_for_user(monitor_id, user_id)
            .await?
            .is_some();

        if !owns_monitor {
            return Ok(None);
        }

        Ok(Some(
            self.channels
                .read()
                .unwrap()
                .values()
                .filter(|channel| channel.monitor_id == monitor_id && channel.user_id == user_id)
                .cloned()
                .collect(),
        ))
    }

    async fn list_enabled_by_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> RepoResult<Vec<NotificationChannel>> {
        Ok(self
            .channels
            .read()
            .unwrap()
            .values()
            .filter(|channel| channel.monitor_id == monitor_id && channel.enabled)
            .cloned()
            .collect())
    }

    async fn list_enabled_email_by_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<NotificationChannel>> {
        Ok(self
            .channels
            .read()
            .unwrap()
            .values()
            .filter(|c| {
                c.tenant_id == tenant_id && c.enabled && c.kind == NotificationChannelKind::Email
            })
            .cloned()
            .collect())
    }

    async fn find_by_id(
        &self,
        id: NotificationChannelId,
    ) -> RepoResult<Option<NotificationChannel>> {
        Ok(self.channels.read().unwrap().get(&id).cloned())
    }

    async fn delete(&self, id: NotificationChannelId) -> RepoResult<bool> {
        Ok(self.channels.write().unwrap().remove(&id).is_some())
    }

    async fn delete_for_tenant(
        &self,
        id: NotificationChannelId,
        tenant_id: TenantId,
    ) -> RepoResult<bool> {
        let mut channels = self.channels.write().unwrap();

        let belongs_to_tenant = channels
            .get(&id)
            .map(|channel| channel.tenant_id == tenant_id)
            .unwrap_or(false);

        if belongs_to_tenant {
            Ok(channels.remove(&id).is_some())
        } else {
            Ok(false)
        }
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<NotificationChannel>> {
        Ok(self
            .channels
            .read()
            .unwrap()
            .values()
            .filter(|channel| channel.monitor_id == monitor_id)
            .cloned()
            .collect())
    }
}
