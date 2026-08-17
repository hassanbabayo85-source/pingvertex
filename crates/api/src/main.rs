use std::net::SocketAddr;
use std::sync::Arc;

use api::{
    auth_state::AuthState,
    config::Config,
    metrics::init_metrics,
    router::build_router,
    state::{AppState, NoopLogoStorage, NoopStatusPageCache},
};
use application::ports::StatusPageCache;
use infra::{
    create_pool, AesGcmProxyCredentialCipher, Argon2PasswordHasher, HickoryCustomDomainVerifier,
    JsonWebTokenService, NotificationDispatcher, PostgresApiFlowRepository,
    PostgresCheckRepository, PostgresDiagnosticReportRepository,
    PostgresEscalationPolicyRepository, PostgresIncidentRepository,
    PostgresMaintenanceWindowRepository, PostgresMonitorRepository,
    PostgresNotificationChannelRepository, PostgresProxyCheckJobRepository,
    PostgresStatusPageRepository, PostgresUserRepository, RedisStatusPageCache, S3LogoStorage,
    UuidIdGenerator,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=trace"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    init_metrics();

    let config = Config::from_env();

    let pool = create_pool(&config.database_url).await?;

    // Status-page response cache: real Redis-backed cache if REDIS_URL is
    // configured, otherwise a no-op cache (fail-open — see NoopStatusPageCache
    // and the StatusPageCache port's doc comment). Self-hosted deployments
    // that don't run Redis still boot fine; public status-page requests
    // just always hit Postgres directly instead of a cache.
    let status_page_cache: Arc<dyn StatusPageCache> = match &config.redis_url {
        Some(redis_url) => Arc::new(RedisStatusPageCache::new(redis_url)?),
        None => {
            tracing::warn!("REDIS_URL not configured — status page responses will not be cached");
            Arc::new(NoopStatusPageCache)
        }
    };

    // Proxy-check credential cipher: fails loudly at startup if
    // PROXY_CREDENTIAL_ENCRYPTION_KEY is missing/invalid, rather than
    // silently falling back to "no encryption" (see
    // AesGcmProxyCredentialCipher::try_from_env's own doc comment).
    let proxy_credential_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let proxy_check_job_repository =
        PostgresProxyCheckJobRepository::new(pool.clone(), proxy_credential_cipher);

    // Logo storage: status-page logo uploads are OPTIONAL. If S3 settings
    // are missing/invalid, fail open and run without logo-upload support
    // (the endpoint will return an error at request time), exactly like
    // Redis/SMTP optional integrations.
    let logo_storage: Arc<dyn application::ports::LogoStorage> = match S3LogoStorage::try_from_env()
    {
        Ok(storage) => Arc::new(storage),
        Err(err) => {
            tracing::warn!(
                error = %err,
                "S3 logo storage not configured; status-page logo uploads will be disabled"
            );
            Arc::new(NoopLogoStorage)
        }
    };

    let app_state = AppState::new_with_feature_repos(
        PostgresMonitorRepository::new(pool.clone()),
        PostgresCheckRepository::new(pool.clone()),
        PostgresDiagnosticReportRepository::new(pool.clone()),
        PostgresIncidentRepository::new(pool.clone()),
        PostgresNotificationChannelRepository::new(pool.clone()),
        NotificationDispatcher::from_env(),
        PostgresApiFlowRepository::new(pool.clone()),
        proxy_check_job_repository,
        PostgresMaintenanceWindowRepository::new(pool.clone()),
        PostgresEscalationPolicyRepository::new(pool.clone()),
        PostgresStatusPageRepository::new(pool.clone()),
        HickoryCustomDomainVerifier::new(),
        status_page_cache,
        logo_storage,
        UuidIdGenerator::new(),
    );

    let auth_state = AuthState::new(
        PostgresUserRepository::new(pool.clone()),
        Argon2PasswordHasher,
        JsonWebTokenService::new(config.jwt_secret.clone()),
        UuidIdGenerator::new(),
    );

    let app = build_router(app_state, auth_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));

    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
