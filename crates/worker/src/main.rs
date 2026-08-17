use std::{error::Error, sync::Arc};

use infra::{
    AesGcmProxyCredentialCipher, HickoryCustomDomainVerifier, NotificationDispatcher,
    PostgresCheckRepository, PostgresDiagnosticReportRepository,
    PostgresEscalationPolicyRepository, PostgresIncidentRepository,
    PostgresMaintenanceWindowRepository, PostgresMonitorRepository,
    PostgresNotificationChannelRepository, PostgresProxyCheckJobRepository,
    PostgresStatusPageRepository, UuidIdGenerator, create_pool,
};
use tracing_subscriber::EnvFilter;
use worker::{
    CustomDomainVerificationWorker, EscalationWorker, IncidentManager, ProxyCheckWorker,
    init_rustls_crypto_provider, metrics, run,
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

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/uptime_monitor".into());

    let pool = create_pool(&database_url).await?;

    let monitor_repository = Arc::new(PostgresMonitorRepository::new(pool.clone()));
    let check_repository = Arc::new(PostgresCheckRepository::new(pool.clone()));
    let incident_repository = Arc::new(PostgresIncidentRepository::new(pool.clone()));
    let notification_channel_repository =
        Arc::new(PostgresNotificationChannelRepository::new(pool.clone()));
    let maintenance_window_repository =
        Arc::new(PostgresMaintenanceWindowRepository::new(pool.clone()));
    let escalation_policy_repository =
        Arc::new(PostgresEscalationPolicyRepository::new(pool.clone()));
    let diagnostic_report_repository =
        Arc::new(PostgresDiagnosticReportRepository::new(pool.clone()));
    let status_page_repository = Arc::new(PostgresStatusPageRepository::new(pool.clone()));
    let custom_domain_verifier = Arc::new(HickoryCustomDomainVerifier::new());
    let id_generator = Arc::new(UuidIdGenerator::new());

    // Proxy check repository requires a cipher for credential encryption.
    let proxy_credential_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let proxy_check_job_repository = Arc::new(PostgresProxyCheckJobRepository::new(
        pool.clone(),
        proxy_credential_cipher,
    ));

    let notification_service = Arc::new(NotificationDispatcher::from_env());

    let incident_manager = IncidentManager::new_with_maintenance_window(
        incident_repository.clone(),
        id_generator.clone(),
        notification_channel_repository.clone(),
        notification_service.clone(),
        maintenance_window_repository.clone(),
    );

    // Spawn escalation worker as a separate task
    let escalation_worker = EscalationWorker::new(
        incident_repository.clone(),
        monitor_repository.clone(),
        escalation_policy_repository,
        notification_channel_repository.clone(),
        notification_service,
    );
    tokio::spawn(async move {
        if let Err(err) = escalation_worker.run_forever().await {
            tracing::error!(error = %err, "escalation worker stopped unexpectedly");
        }
    });

    // Spawn custom-domain verification worker as a separate task.
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

    // Spawn proxy-check worker as a separate task.
    let proxy_check_worker = ProxyCheckWorker::new(proxy_check_job_repository);
    tokio::spawn(async move {
        if let Err(err) = proxy_check_worker.run_forever().await {
            tracing::error!(
                error = %err,
                "proxy check worker stopped unexpectedly"
            );
        }
    });

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build reqwest client");

    run(
        monitor_repository,
        check_repository,
        incident_manager,
        id_generator,
        client,
        diagnostic_report_repository,
    )
    .await?;

    Ok(())
}
