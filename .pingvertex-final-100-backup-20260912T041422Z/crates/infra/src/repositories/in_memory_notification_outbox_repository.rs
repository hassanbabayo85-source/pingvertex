use std::{
    collections::{HashMap, HashSet},
    error::Error,
    sync::{Arc, RwLock},
};

use application::ports::NotificationOutboxRepository;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use domain::{NotificationOutboxEntry, NotificationOutboxEntryId, NotificationOutboxStatus};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Default)]
pub struct InMemoryNotificationOutboxRepository {
    entries: Arc<RwLock<HashMap<NotificationOutboxEntryId, NotificationOutboxEntry>>>,
    idempotency_keys: Arc<RwLock<HashSet<String>>>,
}

impl InMemoryNotificationOutboxRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn find_by_id(&self, id: NotificationOutboxEntryId) -> Option<NotificationOutboxEntry> {
        self.entries.read().unwrap().get(&id).cloned()
    }

    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl NotificationOutboxRepository for InMemoryNotificationOutboxRepository {
    async fn enqueue(&self, entry: &NotificationOutboxEntry) -> RepoResult<bool> {
        let mut keys = self.idempotency_keys.write().unwrap();
        if !keys.insert(entry.idempotency_key.clone()) {
            return Ok(false);
        }

        self.entries
            .write()
            .unwrap()
            .insert(entry.id, entry.clone());
        Ok(true)
    }

    async fn recover_stale_processing(&self, timeout: Duration) -> RepoResult<u64> {
        let cutoff = Utc::now() - timeout;
        let mut entries = self.entries.write().unwrap();
        let mut recovered = 0_u64;

        for entry in entries.values_mut() {
            if entry.status != NotificationOutboxStatus::Processing {
                continue;
            }

            let is_stale = entry
                .processing_started_at
                .map(|started_at| started_at <= cutoff)
                .unwrap_or(false);

            if is_stale {
                entry.status = NotificationOutboxStatus::Pending;
                entry.processing_started_at = None;
                entry.updated_at = Utc::now();
                recovered += 1;
            }
        }

        Ok(recovered)
    }

    async fn claim_next(&self) -> RepoResult<Option<NotificationOutboxEntry>> {
        let now = Utc::now();
        let mut entries = self.entries.write().unwrap();

        let candidate_id = entries
            .values()
            .filter(|entry| {
                entry.status == NotificationOutboxStatus::Pending && entry.retry_at <= now
            })
            .min_by_key(|entry| entry.created_at)
            .map(|entry| entry.id);

        let Some(id) = candidate_id else {
            return Ok(None);
        };

        let entry = entries
            .get_mut(&id)
            .expect("candidate id came from entries");
        let now = Utc::now();
        entry.status = NotificationOutboxStatus::Processing;
        entry.processing_started_at = Some(now);
        entry.updated_at = now;

        Ok(Some(entry.clone()))
    }

    async fn mark_sent(&self, id: NotificationOutboxEntryId) -> RepoResult<()> {
        let mut entries = self.entries.write().unwrap();

        if let Some(entry) = entries.get_mut(&id) {
            entry.status = NotificationOutboxStatus::Sent;
            entry.processing_started_at = None;
            entry.error = None;
            entry.updated_at = Utc::now();
        }

        Ok(())
    }

    async fn mark_failed(
        &self,
        id: NotificationOutboxEntryId,
        error: String,
        retry_at: DateTime<Utc>,
        max_attempts: i32,
    ) -> RepoResult<NotificationOutboxStatus> {
        let mut entries = self.entries.write().unwrap();

        let entry = entries
            .get_mut(&id)
            .ok_or_else(|| -> Box<dyn Error + Send + Sync> { "outbox entry not found".into() })?;

        entry.attempts += 1;
        entry.error = Some(error);
        entry.processing_started_at = None;
        entry.updated_at = Utc::now();

        if entry.attempts >= max_attempts {
            entry.status = NotificationOutboxStatus::Failed;
            Ok(NotificationOutboxStatus::Failed)
        } else {
            entry.status = NotificationOutboxStatus::Pending;
            entry.retry_at = retry_at;
            Ok(NotificationOutboxStatus::Pending)
        }
    }
}
