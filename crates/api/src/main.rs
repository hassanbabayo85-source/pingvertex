use std::net::SocketAddr;

use api::{
    auth_state::AuthState, config::Config, metrics::init_metrics, router::build_router,
    state::AppState,
};
use infra::{
    create_pool, Argon2PasswordHasher, JsonWebTokenService, PostgresCheckRepository,
    PostgresIncidentRepository, PostgresMonitorRepository, PostgresUserRepository, UuidIdGenerator,
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

    axum::serve(listener, app).await?;

    Ok(())
}
