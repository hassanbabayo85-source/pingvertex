//! Worker for recurring proxy monitors.
//!
//! # Overview
//!
//! Every `poll_interval`, this worker claims a batch of due monitors
//! (`RecurringProxyMonitorRepository::claim_due_monitors`), runs a
//! single proxy check per monitor via the pluggable `ProxyCheckRunner`,
//! records the outcome as a `RecurringProxyCheck` row, and reschedules
//! the monitor at its configured interval.
//!
//! # Design decisions
//!
//! * **Synthetic `ProxyEntry`.** The existing check engine
//!   (`crate::proxy_checker::check_single_entry`) takes a `&ProxyEntry`,
//!   but a recurring monitor is not a job entry. The worker builds a
//!   short-lived `ProxyEntry` with fresh dummy ids, runs the check, and
//!   drops it. This avoids duplicating the protocol verification logic
//!   (HTTP/SOCKS4/SOCKS5, TLS, timing) that already lives in the engine.
//!
//! * **Credentials are moved out of the monitor.** `ProxyCredentials`
//!   deliberately does not implement `Clone`. The worker does
//!   `monitor.credentials.take()` before building the entry. After the
//!   check it never needs the credentials again — only `id`, `tenant_id`,
//!   and `interval_seconds`, which remain on the monitor.
//!
//! * **Always reschedule.** A check that "fails" (proxy is dead) is
//!   *data*, not a worker failure. The worker always advances
//!   `next_run_at` by `interval_seconds`, whether the check succeeded,
//!   failed, or timed out. This keeps the schedule deterministic.
//!
//! * **Lease guarded reschedule.** `reschedule` uses
//!   `WHERE worker_id = $3`. If the lease expired and another worker
//!   claimed the monitor in the meantime, our reschedule is a no-op —
//!   we never overwrite the new owner's work.
//!
//! * **Pluggable check runner.** `ProxyCheckRunner` is a trait so
//!   tests can inject a deterministic, network-free implementation.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use application::ports::{
    ClaimedRecurringProxyMonitor, RecurringProxyMonitorRepository,
};
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use domain::entities::{ProxyEntry, RecurringProxyCheck, RecurringProxyMonitor};
use domain::shared::ids::{ProxyCheckJobId, ProxyEntryId};
use domain::value_objects::ProxyResult;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

type BoxError = Box<dyn Error + Send + Sync>;

// ── Pluggable check runner ──────────────────────────────────────────

/// Abstraction over the actual network check.
///
/// Production uses `DefaultProxyCheckRunner`, which delegates to
/// `crate::proxy_checker::check_single_entry`. Tests inject a fake
/// implementation to avoid real network I/O and long waits.
#[async_trait]
pub trait ProxyCheckRunner: Send + Sync {
    async fn run(&self, entry: &ProxyEntry) -> (ProxyResult, bool);
}

/// Production implementation: delegates to the existing proxy check
/// engine.
#[derive(Debug, Clone, Default)]
pub struct DefaultProxyCheckRunner;

#[async_trait]
impl ProxyCheckRunner for DefaultProxyCheckRunner {
    async fn run(&self, entry: &ProxyEntry) -> (ProxyResult, bool) {
        crate::proxy_checker::check_single_entry(entry).await
    }
}

// ── Worker ──────────────────────────────────────────────────────────

pub struct RecurringProxyMonitorWorker {
    repository: Arc<dyn RecurringProxyMonitorRepository>,
    checker: Arc<dyn ProxyCheckRunner>,
    worker_id: String,
    lease_duration: ChronoDuration,
    check_timeout: Duration,
    batch_size: u32,
    poll_interval: Duration,
}

impl RecurringProxyMonitorWorker {
    pub fn new(repository: Arc<dyn RecurringProxyMonitorRepository>) -> Self {
        Self {
            repository,
            checker: Arc::new(DefaultProxyCheckRunner),
            worker_id: Uuid::new_v4().to_string(),
            lease_duration: ChronoDuration::seconds(120),
            check_timeout: Duration::from_secs(60),
            batch_size: 10,
            poll_interval: Duration::from_secs(30),
        }
    }

    pub fn with_checker(mut self, checker: Arc<dyn ProxyCheckRunner>) -> Self {
        self.checker = checker;
        self
    }

    pub fn with_worker_id(mut self, worker_id: impl Into<String>) -> Self {
        self.worker_id = worker_id.into();
        self
    }

    pub fn with_lease_duration(mut self, duration: ChronoDuration) -> Self {
        self.lease_duration = duration;
        self
    }

    pub fn with_check_timeout(mut self, timeout: Duration) -> Self {
        self.check_timeout = timeout;
        self
    }

    pub fn with_batch_size(mut self, size: u32) -> Self {
        self.batch_size = size;
        self
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Claims and processes a single batch. Returns the number of
    /// monitors whose check ran to completion (which may still be a
    /// "failed" check — `Failed` and `TimedOut` are still *completed*
    /// from the worker's perspective).
    pub async fn run_once(&self) -> Result<usize, BoxError> {
        let now = Utc::now();
        let lease_until = now + self.lease_duration;

        let claimed = self
            .repository
            .claim_due_monitors(now, &self.worker_id, lease_until, self.batch_size)
            .await?;

        if claimed.is_empty() {
            return Ok(0);
        }

        let mut completed = 0;
        for c in claimed {
            match self.process_one(c).await {
                Ok(()) => completed += 1,
                Err(e) => {
                    error!(error = %e, "recurring proxy check failed");
                }
            }
        }
        Ok(completed)
    }

    async fn process_one(&self, claimed: ClaimedRecurringProxyMonitor) -> Result<(), BoxError> {
        let mut monitor: RecurringProxyMonitor = claimed.monitor;
        let monitor_id = monitor.id;
        let tenant_id = monitor.tenant_id;
        let interval = monitor.interval_seconds;
        let worker_id = claimed.worker_id;

        info!(
            monitor_id = %monitor_id,
            name = %monitor.name,
            worker_id = %worker_id,
            "running recurring proxy check"
        );

        // Create the history row first, so a crash mid-check leaves
        // evidence that a check was attempted.
        let mut check =
            RecurringProxyCheck::new_running(Uuid::new_v4(), monitor_id, tenant_id);
        self.repository.save_check(&check).await?;

        // Move credentials out of the monitor. After this point the
        // monitor no longer has them, which is fine — we only need
        // id/tenant/interval to reschedule.
        let credentials = monitor.credentials.take();
        let entry = match ProxyEntry::new(
            ProxyEntryId::new(),
            ProxyCheckJobId::new(),
            monitor.host.clone(),
            monitor.port,
            monitor.scheme.clone(),
            credentials,
        ) {
            Ok(e) => e,
            Err(err) => {
                // Bad configuration (e.g. a port that became
                // disallowed by a later policy change). Record it as a
                // failure and still advance the schedule so we don't
                // hot-loop.
                check.finish_failed(format!("invalid proxy configuration: {err}"));
                self.repository.finish_check(&check).await?;
                let now = Utc::now();
                let next = now + ChronoDuration::seconds(i64::from(interval));
                self.repository
                    .reschedule(monitor_id, tenant_id, &worker_id, now, next)
                    .await?;
                return Ok(());
            }
        };

        // Run the check with a hard wall-clock timeout. The engine
        // itself has per-protocol timeouts, but this outer bound
        // protects the worker from any unforeseen hang.
        let timeout_result =
            tokio::time::timeout(self.check_timeout, self.checker.run(&entry)).await;

        match timeout_result {
            Ok((result, _attempted)) => {
                // The `ProxyResult` carries its own outcome field
                // (`Working` / `WorkingTargetUnreachable` /
                // `ProxyFailed`). We record the check as *completed*
                // either way; the UI reads `result.outcome` to decide
                // what to display.
                check.finish_succeeded(result);
            }
            Err(_elapsed) => {
                check.finish_timed_out(format!(
                    "proxy check exceeded worker timeout of {:?}",
                    self.check_timeout
                ));
            }
        }

        if let Err(e) = self.repository.finish_check(&check).await {
            error!(
                error = %e,
                check_id = %check.id,
                monitor_id = %monitor_id,
                "failed to persist terminal check state"
            );
        }

        // Always advance the schedule.
        let now = Utc::now();
        let next = now + ChronoDuration::seconds(i64::from(interval));
        if let Err(e) = self
            .repository
            .reschedule(monitor_id, tenant_id, &worker_id, now, next)
            .await
        {
            error!(
                error = %e,
                monitor_id = %monitor_id,
                "failed to reschedule recurring proxy monitor"
            );
        }

        Ok(())
    }

    /// Runs forever until `token` is cancelled.
    pub async fn run_forever(&self, token: CancellationToken) -> Result<(), BoxError> {
        let mut interval = tokio::time::interval(self.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("recurring proxy monitor worker shutting down");
                    return Ok(());
                }
                _ = interval.tick() => {
                    if let Err(e) = self.run_once().await {
                        error!(error = %e, "recurring proxy monitor worker sweep failed");
                    }
                }
            }
        }
    }
}
