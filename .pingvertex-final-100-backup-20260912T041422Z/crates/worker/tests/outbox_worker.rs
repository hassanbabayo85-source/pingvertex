use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use application::ports::{NotificationOutboxRepository, NotificationOutboxSender};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use domain::{
    NotificationOutboxEntry, NotificationOutboxEntryId, NotificationOutboxEventKind,
    NotificationOutboxStatus, shared::ids::NotificationChannelId,
};
use infra::InMemoryNotificationOutboxRepository;
use uuid::Uuid;
use worker::{OutboxWorker, OutboxWorkerConfig};

fn entry_with_key(key: &str) -> NotificationOutboxEntry {
    NotificationOutboxEntry::new(
        NotificationOutboxEntryId::new(),
        NotificationChannelId(Uuid::new_v4()),
        NotificationOutboxEventKind::MonitorDown,
        "{}".to_string(),
        key.to_string(),
        Utc::now(),
    )
}

#[derive(Clone)]
struct FailingThenSuccessSender {
    remaining_failures: Arc<Mutex<usize>>,
    calls: Arc<Mutex<usize>>,
}

impl FailingThenSuccessSender {
    fn with_failures(count: usize) -> Self {
        Self {
            remaining_failures: Arc::new(Mutex::new(count)),
            calls: Arc::new(Mutex::new(0)),
        }
    }

    fn calls(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl NotificationOutboxSender for FailingThenSuccessSender {
    async fn send(
        &self,
        _entry: &NotificationOutboxEntry,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        *self.calls.lock().unwrap() += 1;

        let mut remaining = self.remaining_failures.lock().unwrap();
        if *remaining > 0 {
            *remaining -= 1;
            return Err(Box::new(std::io::Error::other("simulated send failure")));
        }

        Ok(())
    }
}

fn fast_worker_config() -> OutboxWorkerConfig {
    OutboxWorkerConfig {
        max_concurrent_sends: 1,
        max_attempts: 3,
        base_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(50),
        stale_processing_timeout: Duration::from_secs(1),
        poll_interval: Duration::from_millis(1),
    }
}

#[tokio::test]
async fn retries_with_exponential_backoff_then_succeeds() {
    let repo = Arc::new(InMemoryNotificationOutboxRepository::new());
    let entry = entry_with_key("retry-backoff");
    let id = entry.id;
    repo.enqueue(&entry).await.unwrap();

    let sender = Arc::new(FailingThenSuccessSender::with_failures(2));
    let worker = OutboxWorker::with_config(repo.clone(), sender.clone(), fast_worker_config());

    worker.run_once().await.unwrap();

    let after_first = repo.find_by_id(id).unwrap();
    assert_eq!(after_first.status, NotificationOutboxStatus::Pending);
    assert_eq!(after_first.attempts, 1);
    assert!(after_first.retry_at > Utc::now());

    tokio::time::sleep(Duration::from_millis(30)).await;
    worker.run_once().await.unwrap();

    let after_second = repo.find_by_id(id).unwrap();
    assert_eq!(after_second.status, NotificationOutboxStatus::Pending);
    assert_eq!(after_second.attempts, 2);
    assert!(after_second.retry_at > Utc::now());

    tokio::time::sleep(Duration::from_millis(60)).await;
    worker.run_once().await.unwrap();

    let after_third = repo.find_by_id(id).unwrap();
    assert_eq!(after_third.status, NotificationOutboxStatus::Sent);
    assert_eq!(after_third.attempts, 2);
    assert_eq!(sender.calls(), 3);
}

#[tokio::test]
async fn stale_processing_row_is_recovered_and_sent() {
    let repo = Arc::new(InMemoryNotificationOutboxRepository::new());

    let mut entry = entry_with_key("crash-recovery");
    entry.status = NotificationOutboxStatus::Processing;
    entry.processing_started_at = Some(Utc::now() - ChronoDuration::seconds(5));
    let id = entry.id;
    repo.enqueue(&entry).await.unwrap();

    let sender = Arc::new(FailingThenSuccessSender::with_failures(0));
    let worker = OutboxWorker::with_config(repo.clone(), sender, fast_worker_config());

    worker.run_once().await.unwrap();

    let recovered = repo.find_by_id(id).unwrap();
    assert_eq!(recovered.status, NotificationOutboxStatus::Sent);
    assert_eq!(recovered.processing_started_at, None);
}

#[tokio::test]
async fn duplicate_idempotency_key_is_ignored() {
    let repo = Arc::new(InMemoryNotificationOutboxRepository::new());

    let first = entry_with_key("same-idempotency-key");
    let second = entry_with_key("same-idempotency-key");

    assert!(repo.enqueue(&first).await.unwrap());
    assert!(!repo.enqueue(&second).await.unwrap());
    assert_eq!(repo.len(), 1);
}
