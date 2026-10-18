//! Deterministic, network-free tests for `RecurringProxyMonitorWorker`.
//!
//! The worker's `ProxyCheckRunner` is pluggable, so every test here
//! injects a canned result. No real sockets are opened.

use std::sync::Arc;
use std::time::Duration;

use application::ports::RecurringProxyMonitorRepository;
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use domain::entities::{ProxyEntry, RecurringProxyCheckStatus, RecurringProxyMonitor};
use domain::enums::ProxyScheme;
use domain::shared::ids::{TenantId, UserId};
use domain::value_objects::{ProxyHost, ProxyResult};
use infra::InMemoryRecurringProxyMonitorRepository;
use uuid::Uuid;
use worker::recurring_proxy_monitor_worker::{
    ProxyCheckRunner, RecurringProxyMonitorWorker,
};

// ── Test helpers ────────────────────────────────────────────────────

/// A `ProxyCheckRunner` that always returns the same canned result,
/// after a configurable delay. Fully deterministic.
struct FakeChecker {
    result: ProxyResult,
    delay: Duration,
}

impl FakeChecker {
    fn immediate(result: ProxyResult) -> Arc<Self> {
        Arc::new(Self {
            result,
            delay: Duration::ZERO,
        })
    }

    fn delayed(result: ProxyResult, delay: Duration) -> Arc<Self> {
        Arc::new(Self { result, delay })
    }
}

#[async_trait]
impl ProxyCheckRunner for FakeChecker {
    async fn run(&self, _entry: &ProxyEntry) -> (ProxyResult, bool) {
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        (self.result.clone(), true)
    }
}

fn working_result() -> ProxyResult {
    ProxyResult {
        host: "8.8.8.8".to_string(),
        port: 8080,
        scheme: "http".to_string(),
        reachable: true,
        latency_ms: Some(80),
        anonymity: None,
        supports_https: true,
        country_code: None,
        error_code: None,
        error_message: None,
        outcome: Some(domain::enums::ProxyCheckOutcome::Working),
        failure_kind: None,
        connection_latency_ms: Some(20),
        tls_handshake_latency_ms: Some(30),
        ttfb_ms: Some(60),
        total_time_ms: Some(80),
        http_status_code: Some(200),
    }
}

fn dead_proxy_result() -> ProxyResult {
    ProxyResult {
        host: "8.8.8.8".to_string(),
        port: 8080,
        scheme: "http".to_string(),
        reachable: false,
        latency_ms: None,
        anonymity: None,
        supports_https: false,
        country_code: None,
        error_code: Some("CONNECTION_REFUSED".to_string()),
        error_message: Some("connection refused".to_string()),
        outcome: Some(domain::enums::ProxyCheckOutcome::ProxyFailed),
        failure_kind: None,
        connection_latency_ms: None,
        tls_handshake_latency_ms: None,
        ttfb_ms: None,
        total_time_ms: None,
        http_status_code: None,
    }
}

async fn seed_due_monitor(
    repo: &InMemoryRecurringProxyMonitorRepository,
    tenant: TenantId,
    user: UserId,
    name: &str,
) -> Uuid {
    let mut m = RecurringProxyMonitor::new(
        Uuid::new_v4(),
        tenant,
        user,
        name,
        ProxyHost::new("8.8.8.8").unwrap(),
        8080,
        ProxyScheme::Http,
        None,
        300,
    )
    .unwrap();
    // Make it due right now.
    m.next_run_at = Some(Utc::now() - ChronoDuration::seconds(1));
    let id = m.id;
    repo.save_monitor(&m).await.unwrap();
    id
}

// ── Tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn runs_due_monitor_records_success_and_reschedules() {
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant = TenantId::new();
    let user = UserId::new();
    let id = seed_due_monitor(&repo, tenant, user, "ok").await;

    let worker = RecurringProxyMonitorWorker::new(repo.clone())
        .with_checker(FakeChecker::immediate(working_result()))
        .with_worker_id("test-worker");

    let count = worker.run_once().await.unwrap();
    assert_eq!(count, 1);

    // One check row, Succeeded, with the full result stored.
    let (checks, total) = repo.list_checks(id, tenant, 0, 10).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(checks[0].status, RecurringProxyCheckStatus::Succeeded);
    assert!(checks[0].result.is_some());
    assert!(checks[0].finished_at.is_some());

    // Monitor advanced next_run_at and last_run_at.
    let loaded = repo.find_by_id(id, tenant).await.unwrap().unwrap();
    assert!(loaded.last_run_at.is_some());
    assert!(loaded.next_run_at.unwrap() > Utc::now());
}

#[tokio::test]
async fn records_proxy_failure_as_succeeded_with_outcome() {
    // A dead proxy is data, not a worker failure. The check row is
    // Succeeded; the ProxyResult carries the actual failure details.
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant = TenantId::new();
    let user = UserId::new();
    let id = seed_due_monitor(&repo, tenant, user, "dead").await;

    let worker = RecurringProxyMonitorWorker::new(repo.clone())
        .with_checker(FakeChecker::immediate(dead_proxy_result()));

    let count = worker.run_once().await.unwrap();
    assert_eq!(count, 1);

    let (checks, _) = repo.list_checks(id, tenant, 0, 10).await.unwrap();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].status, RecurringProxyCheckStatus::Succeeded);
    let result = checks[0].result.as_ref().unwrap();
    assert!(!result.reachable);
    assert_eq!(result.error_code.as_deref(), Some("CONNECTION_REFUSED"));
}

#[tokio::test]
async fn records_timeout_as_timed_out() {
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant = TenantId::new();
    let user = UserId::new();
    let id = seed_due_monitor(&repo, tenant, user, "slow").await;

    let worker = RecurringProxyMonitorWorker::new(repo.clone())
        .with_checker(FakeChecker::delayed(working_result(), Duration::from_secs(5)))
        .with_check_timeout(Duration::from_millis(50));

    let count = worker.run_once().await.unwrap();
    assert_eq!(count, 1);

    let (checks, _) = repo.list_checks(id, tenant, 0, 10).await.unwrap();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].status, RecurringProxyCheckStatus::TimedOut);
    assert!(checks[0].error_message.as_ref().unwrap().contains("timeout"));
}

#[tokio::test]
async fn bad_port_is_recorded_as_failed_and_rescheduled() {
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant = TenantId::new();
    let user = UserId::new();

    // Bypass the use-case port validation by inserting directly.
    let mut m = RecurringProxyMonitor::new(
        Uuid::new_v4(),
        tenant,
        user,
        "bad-port",
        ProxyHost::new("8.8.8.8").unwrap(),
        22, // SSH — not an allowed proxy port
        ProxyScheme::Http,
        None,
        300,
    )
    .unwrap();
    m.next_run_at = Some(Utc::now() - ChronoDuration::seconds(1));
    let id = m.id;
    repo.save_monitor(&m).await.unwrap();

    let worker = RecurringProxyMonitorWorker::new(repo.clone())
        .with_checker(FakeChecker::immediate(working_result()));

    let count = worker.run_once().await.unwrap();
    assert_eq!(count, 1);

    let (checks, _) = repo.list_checks(id, tenant, 0, 10).await.unwrap();
    assert_eq!(checks[0].status, RecurringProxyCheckStatus::Failed);
    assert!(checks[0]
        .error_message
        .as_ref()
        .unwrap()
        .contains("invalid proxy configuration"));

    // Still rescheduled.
    let loaded = repo.find_by_id(id, tenant).await.unwrap().unwrap();
    assert!(loaded.next_run_at.unwrap() > Utc::now());
}

#[tokio::test]
async fn empty_batch_returns_zero() {
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let worker = RecurringProxyMonitorWorker::new(repo)
        .with_checker(FakeChecker::immediate(working_result()));

    let count = worker.run_once().await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn disabled_monitor_is_not_processed() {
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant = TenantId::new();
    let user = UserId::new();

    let mut m = RecurringProxyMonitor::new(
        Uuid::new_v4(),
        tenant,
        user,
        "disabled",
        ProxyHost::new("8.8.8.8").unwrap(),
        8080,
        ProxyScheme::Http,
        None,
        300,
    )
    .unwrap();
    m.next_run_at = Some(Utc::now() - ChronoDuration::seconds(1));
    m.set_user_enabled(false);
    repo.save_monitor(&m).await.unwrap();

    let worker = RecurringProxyMonitorWorker::new(repo.clone())
        .with_checker(FakeChecker::immediate(working_result()));

    let count = worker.run_once().await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn suspended_monitor_is_not_processed() {
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant = TenantId::new();
    let user = UserId::new();

    let mut m = RecurringProxyMonitor::new(
        Uuid::new_v4(),
        tenant,
        user,
        "suspended",
        ProxyHost::new("8.8.8.8").unwrap(),
        8080,
        ProxyScheme::Http,
        None,
        300,
    )
    .unwrap();
    m.next_run_at = Some(Utc::now() - ChronoDuration::seconds(1));
    m.set_entitlement_suspended(true);
    repo.save_monitor(&m).await.unwrap();

    let worker = RecurringProxyMonitorWorker::new(repo.clone())
        .with_checker(FakeChecker::immediate(working_result()));

    let count = worker.run_once().await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn processes_batch_of_multiple_due_monitors() {
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant = TenantId::new();
    let user = UserId::new();

    for i in 0..3 {
        seed_due_monitor(&repo, tenant, user, &format!("m{i}")).await;
    }

    let worker = RecurringProxyMonitorWorker::new(repo.clone())
        .with_checker(FakeChecker::immediate(working_result()))
        .with_batch_size(10);

    let count = worker.run_once().await.unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn reschedule_is_noop_after_lease_expired_and_reclaimed() {
    // Simulate: worker A claims, then "dies" (lease expires). Worker B
    // reclaims and reschedules. When worker A comes back and tries to
    // reschedule, it must not overwrite worker B's work.
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let tenant = TenantId::new();
    let user = UserId::new();
    let id = seed_due_monitor(&repo, tenant, user, "lease-race").await;

    // Worker A claims with a very short lease.
    let now = Utc::now();
    let claimed_a = repo
        .claim_due_monitors(now, "worker-a", now + ChronoDuration::milliseconds(10), 1)
        .await
        .unwrap();
    assert_eq!(claimed_a.len(), 1);

    // Wait for the lease to expire.
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Worker B reclaims.
    let later = Utc::now();
    let claimed_b = repo
        .claim_due_monitors(later, "worker-b", later + ChronoDuration::minutes(1), 1)
        .await
        .unwrap();
    assert_eq!(claimed_b.len(), 1);

    // Worker A's reschedule is a no-op: worker_id mismatch.
    let a_ok = repo
        .reschedule(id, tenant, "worker-a", later, later + ChronoDuration::seconds(300))
        .await
        .unwrap();
    assert!(!a_ok);

    // Worker B's reschedule succeeds.
    let b_ok = repo
        .reschedule(id, tenant, "worker-b", later, later + ChronoDuration::seconds(300))
        .await
        .unwrap();
    assert!(b_ok);
}
