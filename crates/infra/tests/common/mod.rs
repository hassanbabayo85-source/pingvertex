use std::env;

use domain::shared::ids::{CheckId, IncidentId, MonitorId, UserId};
use domain::{
    Check, CheckInterval, CheckStatus, ErrorType, ExpectedStatus, HttpMethod, Incident, Monitor,
    MonitorBuilder, MonitorName, Timeout, Url,
};
use infra::create_pool;
use sqlx::{PgPool, query};
use uuid::Uuid;

pub async fn setup_pool() -> PgPool {
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/uptime_monitor".to_string()
    });

    let pool = create_pool(&database_url).await.unwrap();

    println!("CARGO_MANIFEST_DIR = {}", env!("CARGO_MANIFEST_DIR"));

    reset_database(&pool).await;
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    clear_tables(&pool).await;

    pool
}

async fn reset_database(pool: &PgPool) {
    query("DROP SCHEMA IF EXISTS public CASCADE")
        .execute(pool)
        .await
        .unwrap();

    query("CREATE SCHEMA public").execute(pool).await.unwrap();
}

pub async fn clear_tables(pool: &PgPool) {
    query("TRUNCATE TABLE incidents, checks, monitors RESTART IDENTITY CASCADE")
        .execute(pool)
        .await
        .unwrap();
}

pub fn sample_monitor(name: &str, url: &str) -> Monitor {
    MonitorBuilder::new(MonitorId(Uuid::new_v4()), UserId(Uuid::new_v4()))
        .name(MonitorName::new(name).unwrap())
        .url(Url::new(url).unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build()
}

#[allow(dead_code)]
pub fn sample_check(monitor_id: MonitorId) -> Check {
    Check::new(
        CheckId(Uuid::new_v4()),
        monitor_id,
        CheckStatus::Up,
        Some(200),
        123,
        ErrorType::None,
        None,
    )
}

#[allow(dead_code)]
pub fn sample_incident(monitor_id: MonitorId) -> Incident {
    Incident::new(IncidentId(Uuid::new_v4()), monitor_id)
}
