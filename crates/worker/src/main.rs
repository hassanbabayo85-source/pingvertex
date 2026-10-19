mod config;

use std::{error::Error, sync::Arc, time::Duration};

use application::ports::{PaymentProvider, StatusPageCache};
use async_trait::async_trait;
use infra::{
    AesGcmProxyCredentialCipher, BillingConfig, DisabledBillingProvider, FakeBillingProvider,
    HickoryCustomDomainVerifier, NotificationDispatcher, PaystackBillingProvider,
    PostgresApiFlowRepository, PostgresAuditLogEntryRepository,
    PostgresBulkMonitorImportRepository, PostgresCheckRepository,
    PostgresDiagnosticReportRepository, PostgresEscalationPolicyRepository,
    PostgresHistoryRetentionRepository, PostgresIncidentRepository,
    PostgresMaintenanceWindowRepository,     PostgresMonitorRepository, PostgresNotificationChannelRepository,
    PostgresNotificationOutboxRepository, PostgresPAYGPricingRepository,
    PostgresProxyCheckJobRepository,
    PostgresRumRetentionRepository, PostgresRumSiteRepository, PostgresSchedulingRepository,
    PostgresStatusPageRepository, PostgresTenantAlertAllowanceRepository,
    PostgresTenantMembershipRepository, PostgresTenantRepository,
    PostgresTenantRetentionSettingsRepository, PostgresUsageEventRepository, RedisStatusPageCache,
    UuidIdGenerator, create_pool,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use worker::{
    AllowanceResetWorker, AutoTopUpWorker, BulkMonitorActionWorker, BulkMonitorImportWorker,
    CustomDomainVerificationWorker, EscalationWorker, HeartbeatSweeper, IncidentEvaluator,
    IncidentManager, MultiCheckExecutor, OutboxWorker, ProxyCheckWorker,
    RealNotificationOutboxSender, ReconciliationWorker, RetentionWorker, RumAggregationWorker,
    TenantNotificationRateLimiter, WalletExpirationWorker, WalletNotificationEmailWorker,
    init_rustls_crypto_provider, metrics,
};

#[derive(Clone, Default)]
struct NoopStatusPageCache;

#[async_trait]
impl StatusPageCache for NoopStatusPageCache {
    async fn get(&self, _key: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        Ok(None)
    }

    async fn set(
        &self,
        _key: &str,
        _value: &str,
        _ttl_seconds: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    async fn invalidate(&self, _key: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_rustls_crypto_provider();

    // P0 hardening: refuse to boot in production if the private-IP SSRF
    // escape hatch is enabled.
    shared::ssrf::validate_production_ssrf_config()?;

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

    let cancel_token = CancellationToken::new();
    let billing_config = BillingConfig::try_from_env()?;
    let billing_provider: Arc<dyn PaymentProvider> = match billing_config.provider.as_str() {
        "fake" => Arc::new(FakeBillingProvider),
        "disabled" => Arc::new(DisabledBillingProvider),
        "paystack" => Arc::new(PaystackBillingProvider::new(
            billing_config
                .secret_key
                .clone()
                .ok_or("Paystack secret key is required")?,
        )?),
        other => return Err(format!("unsupported billing provider: {other}").into()),
    };

    let monitor_repository = Arc::new(PostgresMonitorRepository::new(pool.clone()));
    let check_repository = Arc::new(PostgresCheckRepository::new(pool.clone()));
    let incident_repository = Arc::new(PostgresIncidentRepository::new(pool.clone()));
    let notification_channel_repository =
        Arc::new(PostgresNotificationChannelRepository::new(pool.clone()));
    let maintenance_window_repository =
        Arc::new(PostgresMaintenanceWindowRepository::new(pool.clone()));
    let escalation_policy_repository =
        Arc::new(PostgresEscalationPolicyRepository::new(pool.clone()));
    let _diagnostic_report_repository =
        Arc::new(PostgresDiagnosticReportRepository::new(pool.clone()));
    let status_page_repository = Arc::new(PostgresStatusPageRepository::new(pool.clone()));
    let custom_domain_verifier = Arc::new(HickoryCustomDomainVerifier::new());
    let id_generator = Arc::new(UuidIdGenerator::new());

    let tenant_repository = Arc::new(PostgresTenantRepository::new(pool.clone()));
    let wallet_repository = Arc::new(infra::PostgresWalletRepository::new(pool.clone()));
    let financial_audit_repository = Arc::new(infra::PostgresFinancialAuditLogRepository::new(
        pool.clone(),
    ));
    let payg_pricing_repository = Arc::new(PostgresPAYGPricingRepository::new(pool.clone()));
    let usage_event_repository = Arc::new(PostgresUsageEventRepository::new(pool.clone()));
    let metering = Arc::new(
        application::MeterUsageUseCase::new(
            wallet_repository.clone(),
            payg_pricing_repository,
            usage_event_repository,
        )
        .with_audit_log_repository(financial_audit_repository.clone())
        .with_tenant_repository(tenant_repository.clone()),
    );
    let bulk_action_worker = BulkMonitorActionWorker::new(
        Arc::new(infra::PostgresBulkMonitorActionRepository::new(
            pool.clone(),
        )),
        monitor_repository.clone(),
        Arc::new(PostgresAuditLogEntryRepository::new(pool.clone())),
    );
    let bulk_action_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = bulk_action_worker.run_forever(bulk_action_cancel).await {
            tracing::error!(error=%err, "bulk monitor action worker stopped unexpectedly");
        }
    });

    let bulk_import_worker = BulkMonitorImportWorker::new(
        Arc::new(PostgresBulkMonitorImportRepository::new(pool.clone())),
        monitor_repository.clone(),
        Arc::new(PostgresTenantMembershipRepository::new(pool.clone())),
        Arc::new(PostgresAuditLogEntryRepository::new(pool.clone())),
        tenant_repository.clone(),
        std::sync::Arc::new(infra::PostgresEntitlementResolver::new(pool.clone())),
    );
    let bulk_import_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = bulk_import_worker.run_forever(bulk_import_cancel).await {
            tracing::error!(error=%err, "bulk monitor import worker stopped unexpectedly");
        }
    });

    let auto_top_up_worker = AutoTopUpWorker::new(
        wallet_repository.clone(),
        billing_provider,
        Duration::from_secs(300),
    )
    .with_monitor_repository(monitor_repository.clone())
    .with_financial_audit_repository(financial_audit_repository.clone());
    let auto_top_up_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = auto_top_up_worker.run_forever(auto_top_up_cancel).await {
            tracing::error!(error=%err,"auto top-up worker stopped unexpectedly");
        }
    });

    let history_retention_repository =
        Arc::new(PostgresHistoryRetentionRepository::new(pool.clone()));

    let proxy_credential_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let proxy_check_job_repository = Arc::new(PostgresProxyCheckJobRepository::new(
        pool.clone(),
        proxy_credential_cipher,
    ));

    let secret_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let allowance_repository = Arc::new(PostgresTenantAlertAllowanceRepository::new(pool.clone()));
    let notification_service = Arc::new(
        NotificationDispatcher::from_env_with_metering(secret_cipher.clone(), metering.clone())
            .with_alert_allowance(
                allowance_repository.clone(),
                monitor_repository.clone(),
                notification_channel_repository.clone(),
            ),
    );
    let wallet_email_worker = WalletNotificationEmailWorker::new(
        wallet_repository.clone(),
        notification_channel_repository.clone(),
        notification_service.clone(),
        Duration::from_secs(60),
    );
    let wallet_email_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = wallet_email_worker.run_forever(wallet_email_cancel).await {
            tracing::error!(error=%err, "wallet notification email worker stopped unexpectedly");
        }
    });

    // GAP 3 Kashi E: RUM raw-event -> daily aggregate rollup and 7-day
    // raw retention. The dashboard reads raw events for the last 7 days
    // and aggregates for older days.
    let rum_site_repository: Arc<dyn application::ports::RumSiteRepository> =
        Arc::new(PostgresRumSiteRepository::new(pool.clone()));
    let rum_retention_repository: Arc<dyn application::ports::RumRetentionRepository> =
        Arc::new(PostgresRumRetentionRepository::new(pool.clone()));
    let rum_aggregation_worker = RumAggregationWorker::new(
        tenant_repository.clone(),
        rum_site_repository.clone(),
        rum_retention_repository,
        Duration::from_secs(6 * 60 * 60),
        1000,
    );
    let rum_aggregation_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = rum_aggregation_worker
            .run_forever(rum_aggregation_cancel)
            .await
        {
            tracing::error!(error=%err, "rum aggregation worker stopped unexpectedly");
        }
    });

    // GAP 3 Kashi D: monthly fair-use allowance reset sweep. Runs hourly
    // by default; the lazy reset in `current_cycle` is the primary path,
    // this worker is a backstop so the dashboard view never drifts.
    let allowance_reset_worker = AllowanceResetWorker::new(
        tenant_repository.clone(),
        monitor_repository.clone(),
        allowance_repository.clone(),
        Duration::from_secs(3600),
    );
    let allowance_reset_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = allowance_reset_worker
            .run_forever(allowance_reset_cancel)
            .await
        {
            tracing::error!(error=%err, "allowance reset worker stopped unexpectedly");
        }
    });

    let api_flow_repository = Arc::new(PostgresApiFlowRepository::new(
        pool.clone(),
        secret_cipher.clone(),
    ));

    let notification_outbox_repository =
        Arc::new(PostgresNotificationOutboxRepository::new(pool.clone()));

    let status_page_cache: Arc<dyn StatusPageCache> = match &worker_config.redis_url {
        Some(redis_url) => Arc::new(RedisStatusPageCache::new(redis_url)?),
        None => {
            tracing::warn!(
                "REDIS_URL not configured — incident open/resolve will not invalidate cached \
                 public status pages immediately (falls back to TTL-only staleness)"
            );
            Arc::new(NoopStatusPageCache)
        }
    };

    let incident_manager = IncidentManager::new_with_maintenance_window(
        incident_repository.clone(),
        id_generator.clone(),
        notification_channel_repository.clone(),
        notification_service.clone(),
        maintenance_window_repository.clone(),
    )
    .with_status_page_cache_invalidation(status_page_repository.clone(), status_page_cache)
    .with_notification_outbox(notification_outbox_repository.clone());

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
    let escalation_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = escalation_worker.run_forever(escalation_cancel).await {
            tracing::error!(error = %err, "escalation worker stopped unexpectedly");
        }
    });

    let custom_domain_verification_worker =
        CustomDomainVerificationWorker::new(status_page_repository, custom_domain_verifier);
    let custom_domain_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = custom_domain_verification_worker
            .run_forever(custom_domain_cancel)
            .await
        {
            tracing::error!(
                error = %err,
                "custom domain verification worker stopped unexpectedly"
            );
        }
    });

    // TASK 5 — metering is now optional on ProxyCheckWorker. Keep it
    // wired during the migration so one-time proxy jobs continue to
    // charge PAYG; TASK 8 will decide whether to remove it permanently.
    let proxy_check_worker = ProxyCheckWorker::new(proxy_check_job_repository)
        .with_metering(metering.clone());
    let proxy_check_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = proxy_check_worker.run_forever(proxy_check_cancel).await {
            tracing::error!(
                error = %err,
                "proxy check worker stopped unexpectedly"
            );
        }
    });

    let reconciliation_worker = ReconciliationWorker::new(pool.clone(), Duration::from_secs(300))
        .with_payg_metering(metering.clone());
    let reconciliation_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = reconciliation_worker
            .run_forever(reconciliation_cancel)
            .await
        {
            tracing::error!(error=%err, "billing reconciliation worker stopped unexpectedly");
        }
    });

    let wallet_expiration_worker =
        WalletExpirationWorker::new(wallet_repository.clone(), Duration::from_secs(300), 200);
    let wallet_expiration_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = wallet_expiration_worker
            .run_forever(wallet_expiration_cancel)
            .await
        {
            tracing::error!(error=%err, "wallet expiration worker stopped unexpectedly");
        }
    });

    let retention_settings_repository =
        Arc::new(PostgresTenantRetentionSettingsRepository::new(pool.clone()));
    // Task 8C — retention is now driven by EntitlementResolver
    // (`max_history_retention_days`); PricingConfig and Wallet are no
    // longer consulted for the retention cutoff.
    let retention_worker = RetentionWorker::new(
        tenant_repository.clone(),
        history_retention_repository,
        std::sync::Arc::new(infra::PostgresEntitlementResolver::new(pool.clone())),
        worker::retention_worker::DEFAULT_BATCH_SIZE,
        worker::retention_worker::DEFAULT_RETENTION_INTERVAL,
        worker_config.retention_max_concurrent_tenants,
        retention_settings_repository,
    );
    let retention_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = retention_worker.run_forever(retention_cancel).await {
            tracing::error!(
                error = %err,
                "retention worker stopped unexpectedly"
            );
        }
    });

    let heartbeat_sweeper = HeartbeatSweeper::new(monitor_repository.clone());
    let heartbeat_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = heartbeat_sweeper
            .run_forever(
                Duration::from_secs(worker_config.lease_seconds / 2),
                heartbeat_cancel,
            )
            .await
        {
            tracing::error!(
                error = %err,
                "heartbeat sweeper stopped unexpectedly"
            );
        }
    });

    let scheduling_repository = Arc::new(PostgresSchedulingRepository::new(pool.clone()));
    let executor = Arc::new(MultiCheckExecutor::new(
        check_repository.clone(),
        id_generator.clone(),
        incident_evaluator,
        api_flow_repository,
    ));

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
    let outbox_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = notification_outbox_worker.run_forever(outbox_cancel).await {
            tracing::error!(
                error = %err,
                "notification outbox worker stopped unexpectedly"
            );
        }
    });

    tracing::info!("worker started; press Ctrl+C or send SIGTERM to shut down");

    tokio::select! {
            result = worker::DurableScheduler::new(
                scheduling_repository,
                executor,
                Duration::from_secs(worker_config.lease_seconds),
                worker_config.max_concurrent_checks,
            )
            .with_metering(metering.clone())
    .with_tenant_repository(tenant_repository.clone())
            .with_entitlement_repositories(
                monitor_repository.clone(),
                std::sync::Arc::new(infra::PostgresEntitlementResolver::new(pool.clone())),
            )
            .with_backpressure(
                worker_config.scheduler_max_concurrency_per_tenant,
                worker_config.scheduler_due_claim_limit_per_tenant,
            )
            .with_deterministic_spread(worker_config.scheduler_deterministic_spread_seconds)
            .with_heartbeat_interval(Duration::from_secs(
                worker_config.heartbeat_interval_seconds,
            ))
            .run() => {
                result?;
            }
            _ = shutdown_signal() => {
                tracing::info!("shutdown signal received, cancelling background workers");
            }
        }

    cancel_token.cancel();
    tracing::info!(
        "cancellation token triggered; waiting briefly for background workers to finish"
    );
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    tracing::info!("worker shut down");

    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
