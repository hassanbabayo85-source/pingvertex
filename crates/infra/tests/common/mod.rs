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
    query(
        "TRUNCATE TABLE notification_channels, incidents, checks, monitors, users RESTART IDENTITY CASCADE",
    )
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

/// Inserts a minimal, valid row into `users` for `user_id`.
///
/// Needed since `migrations/0010_create_notification_channels.sql` added
/// `notification_channels.user_id REFERENCES users(id) ON DELETE CASCADE`
/// — unlike `monitors.user_id`, which has no such FK — so any test that
/// creates a notification channel must first make sure the owning user
/// actually exists in `users`, or the insert fails with a foreign-key
/// violation.
///
/// `email` is derived from `user_id` so it's unique per call even when a
/// test creates more than one user (e.g. an "owner" and an "attacker").
#[allow(dead_code)]
pub async fn insert_user(pool: &PgPool, user_id: UserId) {
    let email = format!("{}@example.test", user_id.0);

    query(
        r#"
        INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
        VALUES ($1, $2, 'not-a-real-hash', 'Test User', NOW(), NOW())
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(user_id.0)
    .bind(email)
    .execute(pool)
    .await
    .unwrap();
}

/// Convenience: builds a monitor AND inserts the matching `users` row for
/// its `user_id`, so callers don't have to remember to call `insert_user`
/// separately every time.
#[allow(dead_code)]
pub async fn sample_monitor_with_user(pool: &PgPool, name: &str, url: &str) -> Monitor {
    let monitor = sample_monitor(name, url);
    insert_user(pool, monitor.user_id).await;
    monitor
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
