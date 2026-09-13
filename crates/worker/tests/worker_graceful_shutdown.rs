//! H16 Task B regression tests: every background worker that accepts a
//! `CancellationToken` must actually stop its `run_forever` loop once
//! that token is cancelled, within a bounded time — not hang forever
//! waiting on the next tick. Each test cancels the token BEFORE the
//! worker starts, then asserts the spawned task completes within a
//! generous timeout, proving the cancellation branch of
//! `tokio::select!` is reachable and correctly shuts the loop down.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use application::ports::{CustomDomainVerifier, IncidentRepository, NotificationService};
use domain::{
    Incident,
    entities::NotificationChannel,
    notification::NotificationEvent,
    shared::ids::{IncidentId, MonitorId},
};
use infra::{
    InMemoryEscalationPolicyRepository, InMemoryHistoryRetentionRepository,
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryNotificationOutboxRepository, InMemoryPricingConfigRepository,
    InMemoryProxyCheckJobRepository, InMemoryStatusPageRepository, InMemoryTenantRepository,
    InMemoryTenantRetentionSettingsRepository, InMemoryWalletRepository,
};
use worker::{
    CustomDomainVerificationWorker, EscalationWorker, HeartbeatSweeper, OutboxWorker,
    ProxyCheckWorker, RetentionWorker,
};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Minimal `IncidentRepository` double. Only `list_open_unacknowledged`
/// (defaulted to `Ok(vec![])`) can actually be reached by
/// `EscalationWorker::run_once` in these tests; the rest are
/// unreachable stubs.
struct NoOpIncidentRepository;

#[async_trait]
impl IncidentRepository for NoOpIncidentRepository {
    async fn save(
        &self,
        _incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn update(
        &self,
        _incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _id: IncidentId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn find_open_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn list_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Vec<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

/// No-op notification sender — never actually reached, since
/// `NoOpIncidentRepository` never returns any incidents to escalate.
struct NoOpNotificationService;

#[async_trait]
impl NotificationService for NoOpNotificationService {
    async fn notify(
        &self,
        _channel: &NotificationChannel,
        _event: &NotificationEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// No-op DNS verifier — never actually reached, since
/// `InMemoryStatusPageRepository::new()` starts with zero pending
/// domains.
struct NoOpCustomDomainVerifier;

#[async_trait]
impl CustomDomainVerifier for NoOpCustomDomainVerifier {
    async fn verify(
        &self,
        _domain: &str,
        _expected_token: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }
}

#[tokio::test]
async fn escalation_worker_stops_gracefully_on_cancellation() {
    let worker = EscalationWorker::new(
        Arc::new(NoOpIncidentRepository),
        Arc::new(InMemoryMonitorRepository::new()),
        Arc::new(InMemoryEscalationPolicyRepository::new()),
        Arc::new(InMemoryNotificationChannelRepository::new(
            InMemoryMonitorRepository::new(),
        )),
        Arc::new(NoOpNotificationService),
    );

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("escalation worker did not shut down within timeout")
        .expect("escalation worker task panicked");

    assert!(
        outcome.is_ok(),
        "escalation worker returned an error on shutdown"
    );
}

#[tokio::test]
async fn custom_domain_verification_worker_stops_gracefully_on_cancellation() {
    let worker = CustomDomainVerificationWorker::new(
        Arc::new(InMemoryStatusPageRepository::new()),
        Arc::new(NoOpCustomDomainVerifier),
    );

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("custom domain verification worker did not shut down within timeout")
        .expect("custom domain verification worker task panicked");

    assert!(
        outcome.is_ok(),
        "custom domain verification worker returned an error on shutdown"
    );
}

#[tokio::test]
async fn proxy_check_worker_stops_gracefully_on_cancellation() {
    let wallet = Arc::new(infra::InMemoryWalletRepository::default());
    let pricing = Arc::new(infra::InMemoryPAYGPricingRepository::default());
    let usage = Arc::new(infra::InMemoryUsageEventRepository::default());
    let metering = Arc::new(application::MeterUsageUseCase::new(wallet, pricing, usage));
    let worker = ProxyCheckWorker::new(Arc::new(InMemoryProxyCheckJobRepository::new()), metering);

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("proxy check worker did not shut down within timeout")
        .expect("proxy check worker task panicked");

    assert!(
        outcome.is_ok(),
        "proxy check worker returned an error on shutdown"
    );
}

#[tokio::test]
async fn heartbeat_sweeper_stops_gracefully_on_cancellation() {
    let sweeper = HeartbeatSweeper::new(Arc::new(InMemoryMonitorRepository::new()));

    let token = CancellationToken::new();
    token.cancel();

    let handle =
        tokio::spawn(async move { sweeper.run_forever(Duration::from_millis(10), token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("heartbeat sweeper did not shut down within timeout")
        .expect("heartbeat sweeper task panicked");

    assert!(
        outcome.is_ok(),
        "heartbeat sweeper returned an error on shutdown"
    );
}

#[tokio::test]
async fn outbox_worker_stops_gracefully_on_cancellation() {
    let worker = OutboxWorker::new(
        Arc::new(InMemoryNotificationOutboxRepository::new()),
        Arc::new(worker::outbox_worker::StubNotificationOutboxSender::new()),
    );

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("outbox worker did not shut down within timeout")
        .expect("outbox worker task panicked");

    assert!(
        outcome.is_ok(),
        "outbox worker returned an error on shutdown"
    );
}

#[tokio::test]
async fn retention_worker_stops_gracefully_on_cancellation() {
    let worker = RetentionWorker::new(
        Arc::new(InMemoryTenantRepository::new()),
        Arc::new(InMemoryHistoryRetentionRepository::new()),
        Arc::new(InMemoryPricingConfigRepository::default()),
        worker::retention_worker::DEFAULT_BATCH_SIZE,
        Duration::from_millis(10),
        2,
        Arc::new(InMemoryWalletRepository::default()),
        Arc::new(InMemoryTenantRetentionSettingsRepository::new()),
    );

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("retention worker did not shut down within timeout")
        .expect("retention worker task panicked");

    assert!(
        outcome.is_ok(),
        "retention worker returned an error on shutdown"
    );
}
