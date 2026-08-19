use std::net::SocketAddr;
use std::sync::Arc;

use api::{
    auth_state::AuthState,
    config::Config,
    metrics::init_metrics,
    mtls::{build_mtls_config, MtlsPaths},
    router::{build_internal_agent_router, build_router},
    state::{AppState, NoopLogoStorage, NoopStatusPageCache},
};
use application::ports::StatusPageCache;
use axum_server::tls_rustls::RustlsAcceptor;
use axum_server_mtls::MtlsAcceptor;
use infra::{
    create_pool, AesGcmProxyCredentialCipher, Argon2PasswordHasher, HickoryCustomDomainVerifier,
    JsonWebTokenService, NotificationDispatcher, PostgresApiFlowRepository,
    PostgresAuditLogEntryRepository, PostgresCheckRepository, PostgresDiagnosticReportRepository,
    PostgresEscalationPolicyRepository, PostgresIncidentRepository,
    PostgresMaintenanceWindowRepository, PostgresMonitorRepository,
    PostgresNotificationChannelRepository, PostgresProxyCheckJobRepository,
    PostgresStatusPageRepository, PostgresTenantMembershipRepository, PostgresUserRepository,
    RedisStatusPageCache, S3LogoStorage, UuidIdGenerator,
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

    // Must run before any `rustls::ServerConfig` is built (i.e. before
    // `build_mtls_config` below) — rustls 0.23 requires a process-wide
    // default `CryptoProvider` to be installed exactly once. We use
    // `aws_lc_rs` (rustls's default feature), matching the crypto
    // provider `mtls.rs` and the rest of the `rustls`/`rustls-pki-types`
    // dependency chain are built against.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install rustls default crypto provider");

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
    let secret_cipher: Arc<dyn application::ports::SecretCipher> = proxy_credential_cipher.clone();
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

    let audit_log_entry_repository = PostgresAuditLogEntryRepository::new(pool.clone());

    let app_state = AppState::new_with_feature_repos(
        PostgresMonitorRepository::new(pool.clone()),
        PostgresCheckRepository::new(pool.clone()),
        PostgresDiagnosticReportRepository::new(pool.clone()),
        PostgresIncidentRepository::new(pool.clone()),
        PostgresNotificationChannelRepository::new(pool.clone()),
        NotificationDispatcher::from_env(secret_cipher.clone()),
        PostgresApiFlowRepository::new(pool.clone()),
        proxy_check_job_repository,
        PostgresMaintenanceWindowRepository::new(pool.clone()),
        PostgresEscalationPolicyRepository::new(pool.clone()),
        PostgresStatusPageRepository::new(pool.clone()),
        HickoryCustomDomainVerifier::new(),
        status_page_cache,
        logo_storage,
        UuidIdGenerator::new(),
        secret_cipher,
        audit_log_entry_repository,
    );

    let tenant_membership_repository = PostgresTenantMembershipRepository::new(pool.clone());

    let auth_state = AuthState::new(
        PostgresUserRepository::new(pool.clone()),
        Argon2PasswordHasher,
        JsonWebTokenService::new(config.jwt_secret.clone()),
        UuidIdGenerator::new(),
        tenant_membership_repository,
    );

    // Two independent servers share `app_state` (Clone, cheap — just Arc
    // clones of the repos/services underneath):
    //
    // 1. Plain HTTP on port 3000 — every route EXCEPT `/internal/v1`.
    // 2. mTLS on `config.mtls_port` — ONLY `/internal/v1` (agent
    //    claim/submit endpoints), requiring every client to present a
    //    certificate signed by our internal CA before any application
    //    code runs. See `router::build_internal_agent_router` and
    //    `mtls::build_mtls_config` for the full rationale.
    //
    // This split (rather than migrating the whole server to
    // `axum_server`) exists because `axum_server` does not expose
    // `ConnectInfo<SocketAddr>`, which the OTHER routes' IP-keyed
    // GovernorLayers depend on.
    let plain_app = build_router(app_state.clone(), auth_state);
    let internal_agent_app = build_internal_agent_router(app_state);

    let plain_addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let mtls_addr = SocketAddr::from(([0, 0, 0, 0], config.mtls_port));

    let mtls_config = build_mtls_config(MtlsPaths {
        ca_cert: &config.mtls_ca_cert_path,
        server_cert: &config.mtls_server_cert_path,
        server_key: &config.mtls_server_key_path,
    })?;

    tracing::info!("Listening on http://{}", plain_addr);
    tracing::info!(
        "Listening (mTLS, internal agents only) on https://{}",
        mtls_addr
    );

    let plain_listener = tokio::net::TcpListener::bind(plain_addr).await?;

    let plain_server = axum::serve(
        plain_listener,
        plain_app.into_make_service_with_connect_info::<SocketAddr>(),
    );

    let mtls_acceptor = MtlsAcceptor::new(RustlsAcceptor::new(mtls_config));

    let mtls_server = axum_server::bind(mtls_addr)
        .acceptor(mtls_acceptor)
        .serve(internal_agent_app.into_make_service());

    // Run both servers concurrently for the lifetime of the process. If
    // either one exits (normally only on an unrecoverable bind/accept
    // error, since neither loop returns on its own otherwise), the whole
    // process exits — a half-running API (only plain HTTP working, or
    // only mTLS working) is worse than a clean crash-and-restart under
    // Docker/orchestrator supervision.
    tokio::try_join!(
        async {
            plain_server
                .await
                .map_err(Box::<dyn std::error::Error>::from)
        },
        async {
            mtls_server
                .await
                .map_err(Box::<dyn std::error::Error>::from)
        },
    )?;

    Ok(())
}
