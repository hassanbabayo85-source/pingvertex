use std::{error::Error, sync::Arc};

use infra::{
    PostgresCheckRepository, PostgresIncidentRepository, PostgresMonitorRepository,
    UuidIdGenerator, create_pool,
};
use tracing_subscriber::EnvFilter;
use worker::{IncidentManager, metrics, run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
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
    let incident_repository = Arc::new(PostgresIncidentRepository::new(pool));
    let id_generator = Arc::new(UuidIdGenerator::new());

    let incident_manager = IncidentManager::new(incident_repository, id_generator.clone());

    let client = reqwest::Client::new();

    run(
        monitor_repository,
        check_repository,
        incident_manager,
        id_generator,
        client,
    )
    .await?;

    Ok(())
}
