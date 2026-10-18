use std::{error::Error, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

use application::ports::{NotificationOutboxRepository, NotificationOutboxSender};
use async_trait::async_trait;
use chrono::Utc;
use domain::NotificationOutboxEntry;
use tokio::{sync::Semaphore, task::JoinSet};
use tracing::{error, info};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub const DEFAULT_MAX_CONCURRENT_SENDS: usize = 5;
pub const DEFAULT_MAX_ATTEMPTS: i32 = 5;
pub const DEFAULT_BASE_BACKOFF: Duration = Duration::from_secs(5);
pub const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(300);
pub const DEFAULT_STALE_PROCESSING_TIMEOUT: Duration = Duration::from_secs(120);
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct OutboxWorkerConfig {
    pub max_concurrent_sends: usize,
    pub max_attempts: i32,
    pub base_backoff: Duration,
    pub max_backoff: Duration,
    pub stale_processing_timeout: Duration,
    pub poll_interval: Duration,
}

impl Default for OutboxWorkerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sends: DEFAULT_MAX_CONCURRENT_SENDS,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            base_backoff: DEFAULT_BASE_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            stale_processing_timeout: DEFAULT_STALE_PROCESSING_TIMEOUT,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

/// Stub outbox sender used by B25.
///
/// It always succeeds. B26/B27 will replace it with real channel senders
/// by implementing `NotificationOutboxSender` and replacing this type in
/// `worker::main`.
#[derive(Debug, Clone, Default)]
pub struct StubNotificationOutboxSender;

impl StubNotificationOutboxSender {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl NotificationOutboxSender for StubNotificationOutboxSender {
    async fn send(&self, entry: &NotificationOutboxEntry) -> Result<()> {
        tracing::info!(
            outbox_entry_id = %entry.id,
            channel_id = %entry.channel_id,
            event_kind = %entry.event_kind,
            "stub outbox sender: pretending to deliver notification"
        );
        Ok(())
    }
}

/// Calculates capped exponential backoff for outbox retries.
///
/// `attempts` is the number of attempts that WILL have occurred once the
/// next delay has elapsed. For example, `attempts = 1` (the first retry)
/// returns `base_backoff`; `attempts = 2` returns `base_backoff * 2`, etc.
pub fn exponential_backoff(
    attempts: i32,
    base_backoff: Duration,
    max_backoff: Duration,
) -> Duration {
    if attempts <= 0 {
        return base_backoff;
    }

    let factor = 2_u64.saturating_pow((attempts - 1) as u32);
    let base_ms = base_backoff.as_millis() as u64;
    let max_ms = max_backoff.as_millis() as u64;
    let delay_ms = base_ms.saturating_mul(factor).min(max_ms);

    Duration::from_millis(delay_ms)
}

pub struct OutboxWorker {
    repository: Arc<dyn NotificationOutboxRepository>,
    sender: Arc<dyn NotificationOutboxSender>,
    config: OutboxWorkerConfig,
}

impl OutboxWorker {
    pub fn new(
        repository: Arc<dyn NotificationOutboxRepository>,
        sender: Arc<dyn NotificationOutboxSender>,
    ) -> Self {
        Self::with_config(repository, sender, OutboxWorkerConfig::default())
    }

    pub fn with_config(
        repository: Arc<dyn NotificationOutboxRepository>,
        sender: Arc<dyn NotificationOutboxSender>,
        config: OutboxWorkerConfig,
    ) -> Self {
        Self {
            repository,
            sender,
            config,
        }
    }

    /// Runs one full sweep: recover stale processing rows, then claim and
    /// deliver every currently-due pending row under a bounded-concurrency
    /// semaphore.
    pub async fn run_once(&self) -> Result<()> {
        let stale_timeout = chrono::Duration::from_std(self.config.stale_processing_timeout)?;
        self.repository
            .recover_stale_processing(stale_timeout)
            .await?;

        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_sends));
        let mut join_set = JoinSet::new();

        loop {
            let permit = semaphore.clone().acquire_owned().await?;
            let entry = self.repository.claim_next().await?;

            let Some(entry) = entry else {
                break;
            };

            let repository = self.repository.clone();
            let sender = self.sender.clone();
            let max_attempts = self.config.max_attempts;
            let base_backoff = self.config.base_backoff;
            let max_backoff = self.config.max_backoff;

            join_set.spawn(async move {
                let _permit = permit;

                match sender.send(&entry).await {
                    Ok(()) => {
                        if let Err(err) = repository.mark_sent(entry.id).await {
                            error!(
                                outbox_entry_id = %entry.id,
                                error = %err,
                                "failed to mark outbox entry as sent"
                            );
                        }
                    }
                    Err(send_err) => {
                        let attempts_after_failure = entry.attempts + 1;
                        let delay =
                            exponential_backoff(attempts_after_failure, base_backoff, max_backoff);

                        let retry_at = match chrono::Duration::from_std(delay) {
                            Ok(delta) => Utc::now() + delta,
                            Err(err) => {
                                error!(
                                    outbox_entry_id = %entry.id,
                                    error = %err,
                                    "failed to convert outbox backoff duration"
                                );
                                return;
                            }
                        };

                        if let Err(mark_err) = repository
                            .mark_failed(entry.id, send_err.to_string(), retry_at, max_attempts)
                            .await
                        {
                            error!(
                                outbox_entry_id = %entry.id,
                                error = %mark_err,
                                "failed to record outbox delivery failure"
                            );
                        }
                    }
                }
            });
        }

        while join_set.join_next().await.is_some() {}

        Ok(())
    }

    /// Runs forever, sweeping once every `config.poll_interval`.
    pub async fn run_forever(&self, token: CancellationToken) -> Result<()> {
        let mut ticker = tokio::time::interval(self.config.poll_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("notification outbox worker cancellation requested; shutting down");
                    return Ok(());
                }
                _ = ticker.tick() => {
                    if let Err(err) = self.run_once().await {
                        error!(error = %err, "notification outbox sweep failed");
                    }
                }
            }
        }
    }
}
