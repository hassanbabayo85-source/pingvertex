use std::net::SocketAddr;

use api::{
    auth_state::AuthState, config::Config, metrics::init_metrics, router::build_router,
    state::AppState,
};
use infra::{
    create_pool, Argon2PasswordHasher, JsonWebTokenService, NotificationDispatcher,
    PostgresApiFlowRepository, PostgresCheckRepository, PostgresIncidentRepository,
    PostgresMonitorRepository, PostgresNotificationChannelRepository, PostgresUserRepository,
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

    let app_state = AppState::new(
        PostgresMonitorRepository::new(pool.clone()),
        PostgresCheckRepository::new(pool.clone()),
        PostgresIncidentRepository::new(pool.clone()),
        PostgresNotificationChannelRepository::new(pool.clone()),
        // Shared across every request. Reads SMTP_* env vars once at
        // startup so the "send test notification" endpoint and the
        // worker's IncidentManager use identical delivery logic — see
        // `NotificationDispatcher::from_env`.
        NotificationDispatcher::from_env(),
        PostgresApiFlowRepository::new(pool.clone()),
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
