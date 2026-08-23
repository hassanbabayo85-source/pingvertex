mod config;

use std::{error::Error, sync::Arc, time::Duration};

use infra::{
    AesGcmProxyCredentialCipher, HickoryCustomDomainVerifier, NotificationDispatcher,
    PostgresApiFlowRepository, PostgresCheckRepository, PostgresDiagnosticReportRepository,
    PostgresEscalationPolicyRepository, PostgresHistoryRetentionRepository,
    PostgresIncidentRepository, PostgresMaintenanceWindowRepository, PostgresMonitorRepository,
    PostgresNotificationChannelRepository, PostgresNotificationOutboxRepository,
    PostgresProxyCheckJobRepository, PostgresSchedulingRepository, PostgresStatusPageRepository,
    PostgresSubscriptionRepository, PostgresTenantRepository, UuidIdGenerator, create_pool,
};
use tracing_subscriber::EnvFilter;
use worker::{
    CustomDomainVerificationWorker, EscalationWorker, HeartbeatSweeper, IncidentEvaluator,
    IncidentManager, MultiCheckExecutor, OutboxWorker, ProxyCheckWorker,
    RealNotificationOutboxSender, RetentionWorker, TenantNotificationRateLimiter,
    init_rustls_crypto_provider, metrics, run_durable,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_rustls_crypto_provider();
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=trace"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    metrics::init_metrics();

    tokio::spawn(async {
        if let Err(err) = metrics::serve_metrics().await {
            tracing::error!(error = %err, "worker metrics server failed");
        }
    });

    let worker_config = config::WorkerConfig::from_env();

    let pool = create_pool(&worker_config.database_url).await?;

    let monitor_repository = Arc::new(PostgresMonitorRepository::new(pool.clone()));
    let check_repository = Arc::new(PostgresCheckRepository::new(pool.clone()));
    let incident_repository = Arc::new(PostgresIncidentRepository::new(pool.clone()));
    let notification_channel_repository =
        Arc::new(PostgresNotificationChannelRepository::new(pool.clone()));
    let maintenance_window_repository =
        Arc::new(PostgresMaintenanceWindowRepository::new(pool.clone()));
    let escalation_policy_repository =
        Arc::new(PostgresEscalationPolicyRepository::new(pool.clone()));
    // Reserved for a future worker that wires diagnostics/incident
    // management into the durable scheduler's active check path.
    let _diagnostic_report_repository =
        Arc::new(PostgresDiagnosticReportRepository::new(pool.clone()));
    let status_page_repository = Arc::new(PostgresStatusPageRepository::new(pool.clone()));
    let custom_domain_verifier = Arc::new(HickoryCustomDomainVerifier::new());
    let id_generator = Arc::new(UuidIdGenerator::new());

    let tenant_repository = Arc::new(PostgresTenantRepository::new(pool.clone()));
    let subscription_repository = Arc::new(PostgresSubscriptionRepository::new(pool.clone()));
    let history_retention_repository =
        Arc::new(PostgresHistoryRetentionRepository::new(pool.clone()));

    let proxy_credential_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let proxy_check_job_repository = Arc::new(PostgresProxyCheckJobRepository::new(
        pool.clone(),
        proxy_credential_cipher,
    ));

    let secret_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let notification_service = Arc::new(NotificationDispatcher::from_env(secret_cipher));

    // B29: API flow definitions (multi-step HTTP flows), read by
    // `FlowCheckExecutor` (via `MultiCheckExecutor`) at check time.
    let api_flow_repository = Arc::new(PostgresApiFlowRepository::new(pool.clone()));

    // B23: incident state machine, wired into the durable scheduler's
    // active check path via `IncidentEvaluator` below. The legacy sweep
    // scheduler (`worker::run`, not currently invoked from `main`) still
    // constructs and uses its own separate `IncidentManager` instance.
    let incident_manager = IncidentManager::new_with_maintenance_window(
        incident_repository.clone(),
        id_generator.clone(),
        notification_channel_repository.clone(),
        notification_service.clone(),
        maintenance_window_repository.clone(),
    );

    let incident_evaluator = IncidentEvaluator::new(
        monitor_repository.clone(),
        incident_manager,
        worker_config.incident_failure_threshold,
        worker_config.incident_recovery_threshold,
    );

    let escalation_worker = EscalationWorker::new(
        incident_repository.clone(),
        monitor_repository.clone(),
        escalation_policy_repository,
        notification_channel_repository.clone(),
        notification_service.clone(),
    );
    tokio::spawn(async move {
        if let Err(err) = escalation_worker.run_forever().await {
            tracing::error!(error = %err, "escalation worker stopped unexpectedly");
        }
    });

    let custom_domain_verification_worker =
        CustomDomainVerificationWorker::new(status_page_repository, custom_domain_verifier);
    tokio::spawn(async move {
        if let Err(err) = custom_domain_verification_worker.run_forever().await {
            tracing::error!(
                error = %err,
                "custom domain verification worker stopped unexpectedly"
            );
        }
    });

    let proxy_check_worker = ProxyCheckWorker::new(proxy_check_job_repository);
    tokio::spawn(async move {
        if let Err(err) = proxy_check_worker.run_forever().await {
            tracing::error!(
                error = %err,
                "proxy check worker stopped unexpectedly"
            );
        }
    });

    // B22: Tenant-scoped history retention enforcement. Runs on its own
    // interval, using batched deletes, independent of the check scheduler.
    let retention_worker = RetentionWorker::new(
        tenant_repository,
        subscription_repository,
        history_retention_repository,
        worker::retention_worker::DEFAULT_BATCH_SIZE,
        worker::retention_worker::DEFAULT_RETENTION_INTERVAL,
    );
    tokio::spawn(async move {
        if let Err(err) = retention_worker.run_forever().await {
            tracing::error!(
                error = %err,
                "retention worker stopped unexpectedly"
            );
        }
    });

    // B19: Passive heartbeat sweep. Runs on its own interval and does NOT
    // participate in the claim-based active scheduler below.
    let heartbeat_sweeper = HeartbeatSweeper::new(monitor_repository.clone());
    tokio::spawn(async move {
        if let Err(err) = heartbeat_sweeper
            .run_forever(Duration::from_secs(worker_config.lease_seconds / 2))
            .await
        {
            tracing::error!(
                error = %err,
                "heartbeat sweeper stopped unexpectedly"
            );
        }
    });

    let scheduling_repository = Arc::new(PostgresSchedulingRepository::new(pool.clone()));
    // B19: Dispatcher handles HTTP (B18), TCP, and DNS. B29 adds Flow.
    // Heartbeat is intentionally excluded from the claim query, so it
    // never reaches this executor. B23: each sub-executor now also
    // evaluates incident state (consecutive-failure/success thresholds)
    // via `incident_evaluator` after persisting its check.
    let executor = Arc::new(MultiCheckExecutor::new(
        check_repository.clone(),
        id_generator.clone(),
        incident_evaluator,
        api_flow_repository,
    ));

    // B25: Notification outbox reliability layer. Uses a stub sender for
    // now; B26/B27 will replace the sender with real channel delivery.
    let notification_outbox_repository =
        Arc::new(PostgresNotificationOutboxRepository::new(pool.clone()));
    let tenant_notification_rate_limiter = Arc::new(TenantNotificationRateLimiter::new(
        60,
        Duration::from_secs(60),
    ));
    let notification_outbox_sender = Arc::new(RealNotificationOutboxSender::new(
        notification_service.clone(),
        notification_channel_repository.clone(),
        tenant_notification_rate_limiter,
    ));
    let notification_outbox_worker =
        OutboxWorker::new(notification_outbox_repository, notification_outbox_sender);
    tokio::spawn(async move {
        if let Err(err) = notification_outbox_worker.run_forever().await {
            tracing::error!(
                error = %err,
                "notification outbox worker stopped unexpectedly"
            );
        }
    });

    run_durable(
        scheduling_repository,
        executor,
        Duration::from_secs(worker_config.lease_seconds),
        worker_config.max_concurrent_checks,
    )
    .await?;

    Ok(())
}
