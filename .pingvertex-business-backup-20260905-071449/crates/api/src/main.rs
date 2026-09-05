use std::net::SocketAddr;
use std::sync::Arc;

use api::{
    auth_state::{AuthState, NoopPendingAuthCache},
    config::Config,
    metrics::init_metrics,
    mtls::{MtlsPaths, build_mtls_config},
    router::{build_internal_agent_router, build_router},
    state::{AppState, NoopLogoStorage, NoopStatusPageCache},
};
use application::ports::{
    EmailVerificationMailer, IdTokenVerifier, PasswordResetMailer, PaymentProvider, StatusPageCache,
};
use axum_server::tls_rustls::RustlsAcceptor;
use axum_server_mtls::MtlsAcceptor;
use infra::services::MaxMindGeoIpLookup;
use infra::{
    AesGcmProxyCredentialCipher, Argon2PasswordHasher, BillingConfig, DisabledBillingProvider,
    DisabledEmailVerificationMailer, DisabledIdTokenVerifier, DisabledPasswordResetMailer,
    FakeBillingProvider, HickoryCustomDomainVerifier, JsonWebTokenService, NotificationDispatcher,
    OidcIdTokenVerifier, PostgresAddOnSubscriptionRepository, PostgresAgentLeaseRepository,
    PostgresApiFlowRepository, PostgresApiKeyRepository, PostgresAuditLogEntryRepository,
    PostgresBackupCodeRepository, PostgresBillingAuditLogRepository,
    PostgresBillingWebhookIdempotencyRepository, PostgresCheckRepository,
    PostgresCheckoutSessionRepository, PostgresDashboardRepository,
    PostgresDiagnosticReportRepository, PostgresEmailVerificationTokenRepository,
    PostgresEscalationPolicyRepository, PostgresHealthCheck, PostgresHelpArticleRepository,
    PostgresHelpCategoryRepository, PostgresIncidentRepository,
    PostgresMaintenanceWindowRepository, PostgresMonitorRepository,
    PostgresNotificationChannelRepository, PostgresPricingConfigRepository,
    PostgresProxyCheckJobRepository, PostgresRegistrationTransaction,
    PostgresResetPasswordTokenRepository, PostgresRumAnalyticsRepository,
    PostgresRumEventRepository, PostgresRumSiteRepository, PostgresStatusPageRepository,
    PostgresSubscriptionRepository, PostgresSubscriptionUsagePeriodRepository,
    PostgresSubscriptionUsageTransaction, PostgresSupportTicketRepository,
    PostgresTenantInvitationRepository, PostgresTenantMembershipRepository,
    PostgresTenantRepository, PostgresUserRepository, RedisPendingAuthCache, RedisRateLimiter,
    RedisStatusPageCache, S3LogoStorage, SmtpConfig, SmtpEmailVerificationMailer,
    SmtpPasswordResetMailer, TwilioPricingService, UuidIdGenerator, create_pool,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // P0 hardening: refuse to boot in production if the private-IP SSRF
    // escape hatch is enabled.
    shared::ssrf::validate_production_ssrf_config()?;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=trace"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install rustls default crypto provider");

    init_metrics();

    let config = Config::from_env();

    // Session invalidation (logout/password changes) and TOTP's pending
    // authentication state are security-sensitive. In production they must
    // have a real shared cache; the test/development no-op implementation
    // would otherwise make revocation silently ineffective.
    let environment = std::env::var("ENVIRONMENT").unwrap_or_default();
    if matches!(
        environment.to_ascii_lowercase().as_str(),
        "production" | "prod"
    ) && config.redis_url.is_none()
    {
        return Err(
            "REDIS_URL must be configured in production for session invalidation and TOTP".into(),
        );
    }

    let pool = create_pool(&config.database_url).await?;

    let billing_config = BillingConfig::try_from_env()?;
    let billing_provider: Arc<dyn PaymentProvider> = match billing_config.provider.as_str() {
        "fake" => Arc::new(FakeBillingProvider),
        "disabled" => Arc::new(DisabledBillingProvider),
        other => panic!("unsupported billing provider: {other}"),
    };

    let status_page_cache: Arc<dyn StatusPageCache> = match &config.redis_url {
        Some(redis_url) => Arc::new(RedisStatusPageCache::new(redis_url)?),
        None => {
            tracing::warn!("REDIS_URL not configured — status page responses will not be cached");
            Arc::new(NoopStatusPageCache)
        }
    };

    let proxy_credential_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let secret_cipher: Arc<dyn application::ports::SecretCipher> = proxy_credential_cipher.clone();
    let proxy_check_job_repository =
        PostgresProxyCheckJobRepository::new(pool.clone(), proxy_credential_cipher);

    let pending_auth_cache: Arc<dyn application::ports::PendingAuthCache> = match &config.redis_url
    {
        Some(redis_url) => Arc::new(RedisPendingAuthCache::new(redis_url)?),
        None => {
            tracing::warn!(
                "REDIS_URL not configured — TOTP 2FA enrollment/login AND B6 password-reset \
                 session invalidation will not work (pending secrets/tokens and session-\
                 invalidation markers have nowhere to be stored)"
            );
            Arc::new(NoopPendingAuthCache)
        }
    };

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

    let secret_cipher_for_auth = secret_cipher.clone();

    // Shared distributed rate limiter. Constructed once and cloned into
    // both `AppState` (RUM ingestion) and `AuthState` (auth endpoints)
    // below — `RedisRateLimiter` is `Clone` (cheap: it only wraps a
    // `redis::Client`, which is itself a lightweight connection-info
    // handle, not an open connection) and both states key their own
    // requests under distinct namespaces ("rum_ingest" vs
    // "forgot_password"/"resend_verification"), so sharing one instance
    // is equivalent to constructing two and cannot cross-contaminate
    // either's counters.
    let redis_rate_limiter = RedisRateLimiter::new(
        &std::env::var("REDIS_URL")
            .map_err(|_| anyhow::anyhow!("REDIS_URL is required for rate limiting"))?,
    )?;

    let mut app_state = AppState::new_with_feature_repos(
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
    )
    .with_api_key_repository(PostgresApiKeyRepository::new(pool.clone()))
    .with_subscription_repository(PostgresSubscriptionRepository::new(pool.clone()))
    .with_checkout_session_repository(PostgresCheckoutSessionRepository::new(pool.clone()))
    .with_payment_provider(billing_provider)
    .with_billing_webhook_idempotency_repository(PostgresBillingWebhookIdempotencyRepository::new(
        pool.clone(),
    ))
    .with_billing_audit_log_repository(PostgresBillingAuditLogRepository::new(pool.clone()))
    .with_agent_lease_repository(PostgresAgentLeaseRepository::new(pool.clone()))
    .with_health_check_port(PostgresHealthCheck::new(pool.clone()))
    .with_dashboard_repository(PostgresDashboardRepository::new(pool.clone()))
    .with_pricing_config_repository(PostgresPricingConfigRepository::new(pool.clone()))
    .with_add_on_subscription_repository(PostgresAddOnSubscriptionRepository::new(pool.clone()))
    .with_sms_provider_pricing(TwilioPricingService::new())
    .with_subscription_usage_period_repository(PostgresSubscriptionUsagePeriodRepository::new(
        pool.clone(),
    ))
    .with_subscription_usage_transaction(PostgresSubscriptionUsageTransaction::new(pool.clone()))
    .with_help_category_repository(PostgresHelpCategoryRepository::new(pool.clone()))
    .with_help_article_repository(PostgresHelpArticleRepository::new(pool.clone()))
    .with_support_ticket_repository(PostgresSupportTicketRepository::new(pool.clone()))
    .with_rum_site_repository(PostgresRumSiteRepository::new(pool.clone()))
    .with_rum_event_repository(PostgresRumEventRepository::new(pool.clone()))
    .with_rum_analytics_repository(PostgresRumAnalyticsRepository::new(pool.clone()))
    .with_rate_limiter(redis_rate_limiter.clone());

    // Geo-IP lookup for RUM sessions is best-effort: reuses the same
    // GeoLite2 database `crates/agent` already requires, but unlike the
    // agent (where a missing DB is a fatal startup error), a missing/
    // unreadable DB here must NOT stop the API server from starting —
    // RUM ingestion is fully functional without it, just with
    // `country_code = None` on every session (see `NoopGeoIpLookup` in
    // `state.rs`). Defaults to the same path `crates/agent` defaults to,
    // so a single `GEOIP_DB_PATH` env var (or the same default file
    // location) covers both processes.
    let geoip_db_path = std::env::var("GEOIP_DB_PATH")
        .unwrap_or_else(|_| "data/geoip/GeoLite2-Country.mmdb".to_string());
    match MaxMindGeoIpLookup::open(&geoip_db_path) {
        Ok(geo_ip_lookup) => {
            app_state = app_state.with_geo_ip_lookup(geo_ip_lookup);
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                path = %geoip_db_path,
                "RUM geo-IP database unavailable; RUM sessions will have no country_code until this is fixed"
            );
        }
    }

    let app_state = match billing_config.webhook_secret {
        Some(secret) => app_state.with_billing_webhook_secret(secret),
        None => app_state,
    };

    let tenant_repository = PostgresTenantRepository::new(pool.clone());
    let tenant_membership_repository = PostgresTenantMembershipRepository::new(pool.clone());
    let registration_transaction = PostgresRegistrationTransaction::new(pool.clone());

    let email_verification_token_repository =
        PostgresEmailVerificationTokenRepository::new(pool.clone());

    let email_verification_mailer: Arc<dyn EmailVerificationMailer> = match SmtpConfig::try_from_env(
    ) {
        Ok(smtp_config) => match SmtpEmailVerificationMailer::new(smtp_config) {
            Ok(mailer) => Arc::new(mailer),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "failed to build verification email mailer; verification emails will be disabled"
                );
                Arc::new(DisabledEmailVerificationMailer::new(err.to_string()))
            }
        },
        Err(err) => {
            tracing::warn!(
                error = %err,
                "SMTP not configured; verification emails will be disabled"
            );
            Arc::new(DisabledEmailVerificationMailer::new(err.to_string()))
        }
    };

    let reset_password_token_repository = PostgresResetPasswordTokenRepository::new(pool.clone());

    let password_reset_mailer: Arc<dyn PasswordResetMailer> = match SmtpConfig::try_from_env() {
        Ok(smtp_config) => match SmtpPasswordResetMailer::new(smtp_config) {
            Ok(mailer) => Arc::new(mailer),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "failed to build password reset mailer; password reset emails will be disabled"
                );
                Arc::new(DisabledPasswordResetMailer::new(err.to_string()))
            }
        },
        Err(err) => {
            tracing::warn!(
                error = %err,
                "SMTP not configured; password reset emails will be disabled"
            );
            Arc::new(DisabledPasswordResetMailer::new(err.to_string()))
        }
    };

    let invitation_repository = PostgresTenantInvitationRepository::new(pool.clone());

    // OAuth — Continue with Google / Apple. Both are optional: a
    // deployment that only wants email/password auth doesn't need to
    // set these. Same "fail loud, not silently disabled" shape as the
    // SMTP mailers above — a missing client ID yields a
    // `DisabledIdTokenVerifier` that clearly refuses every request
    // instead of pretending to work.
    let google_id_token_verifier: Arc<dyn IdTokenVerifier> = match std::env::var("GOOGLE_CLIENT_ID")
    {
        Ok(client_id) if !client_id.trim().is_empty() => {
            Arc::new(OidcIdTokenVerifier::google(client_id))
        }
        _ => {
            tracing::warn!("GOOGLE_CLIENT_ID not set; Google sign-in will be disabled");
            Arc::new(DisabledIdTokenVerifier::new("Google"))
        }
    };

    let apple_id_token_verifier: Arc<dyn IdTokenVerifier> = match std::env::var("APPLE_CLIENT_ID") {
        Ok(client_id) if !client_id.trim().is_empty() => {
            Arc::new(OidcIdTokenVerifier::apple(client_id))
        }
        _ => {
            tracing::warn!("APPLE_CLIENT_ID not set; Apple sign-in will be disabled");
            Arc::new(DisabledIdTokenVerifier::new("Apple"))
        }
    };

    let auth_state = AuthState::new(
        PostgresUserRepository::new(pool.clone()),
        Argon2PasswordHasher,
        JsonWebTokenService::new(config.jwt_secret.clone()),
        UuidIdGenerator::new(),
        tenant_repository,
        tenant_membership_repository,
        registration_transaction,
    )
    .with_totp_support(
        secret_cipher_for_auth,
        pending_auth_cache,
        PostgresBackupCodeRepository::new(pool.clone()),
    )
    .with_rate_limiter(redis_rate_limiter)
    .with_email_verification(
        email_verification_token_repository,
        email_verification_mailer,
    )
    .with_password_reset(reset_password_token_repository, password_reset_mailer)
    .with_invitation_repository(invitation_repository)
    .with_oauth(google_id_token_verifier, apple_id_token_verifier);

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
    )
    .with_graceful_shutdown(shutdown_signal());

    let mtls_acceptor = MtlsAcceptor::new(RustlsAcceptor::new(mtls_config));

    let mtls_handle = axum_server::Handle::new();
    let shutdown_handle = mtls_handle.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown signal received, draining mTLS listener");
        shutdown_handle.graceful_shutdown(Some(std::time::Duration::from_secs(25)));
    });

    let mtls_server = axum_server::bind(mtls_addr)
        .acceptor(mtls_acceptor)
        .handle(mtls_handle)
        .serve(internal_agent_app.into_make_service());

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

    tracing::info!("api shut down cleanly");

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
