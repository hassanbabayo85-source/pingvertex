mod common;

use application::ports::PricingConfigRepository;
use domain::pricing::PricingConfig;
use domain::shared::ids::UserId;
use infra::{PostgresPricingConfigRepository, create_pool};
use sqlx::PgPool;
use std::env;
use uuid::Uuid;

/// Sets up a pool against DATABASE_URL and runs migrations, but
/// deliberately does NOT call `common::clear_tables()`.
///
/// `clear_tables()` does `TRUNCATE ... users ... CASCADE`, and since
/// `pricing_configs.created_by` has a foreign key to `users`, that
/// CASCADE would also wipe `pricing_configs` — including the version 1
/// row inserted by the `0055_seed_initial_pricing_config.sql` migration
/// that this test needs to assert against. Instead, we only clean up any
/// leftover version-2 rows from a previous run of this specific test, so
/// re-running it is idempotent without disturbing the seeded version 1.
async fn setup_pricing_pool() -> PgPool {
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/uptime_monitor".to_string()
    });
    let pool = create_pool(&database_url)
        .await
        .expect("failed to create pool");

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    // Idempotent cleanup: remove any version 2 rows left over from a
    // previous run of this test (child table first, to respect FKs).
    sqlx::query("DELETE FROM pricing_audit_logs WHERE version = 2")
        .execute(&pool)
        .await
        .expect("cleanup of pricing_audit_logs failed");
    sqlx::query("DELETE FROM pricing_configs WHERE version = 2")
        .execute(&pool)
        .await
        .expect("cleanup of pricing_configs failed");

    pool
}

#[tokio::test]
#[ignore] // Requires a real PostgreSQL database (set DATABASE_URL)
async fn test_seed_and_audit_log_flow() {
    let pool = setup_pricing_pool().await;
    let repo = PostgresPricingConfigRepository::new(pool.clone());

    // (a) Seed migration ya samar da version 1
    let active = repo.find_active_config().await.expect("query failed");
    assert!(active.is_some(), "Expected a seeded version 1");
    let active = active.unwrap();
    assert_eq!(active.version, 1);
    assert_eq!(active.currency, "USD");

    // (b) created_by ya rubuta daidai lokacin ƙirƙirar sabon version.
    // `created_by` is a real foreign key to `users`, so we must insert an
    // actual user row first (reusing the shared test helper) rather than
    // using a random UUID.
    let user_id = UserId::new();
    let _tenant_id = common::insert_user(&pool, user_id).await;

    let mut new_config = PricingConfig::default_config();
    new_config.version = 2;
    new_config.created_by = Some(user_id);
    new_config.effective_at = chrono::Utc::now() + chrono::Duration::minutes(1); // nan gaba

    let reason = Some("test change".to_string());
    repo.create_version(&new_config, reason.clone())
        .await
        .expect("create_version failed");

    let stored = repo
        .find_by_version(2)
        .await
        .expect("query failed")
        .expect("version 2 not found");
    assert_eq!(stored.created_by, Some(user_id));

    // (c) Sabon version yana haifar da audit log row daidai.
    let audit_row: (
        String,
        Option<String>,
        Option<String>,
        Option<Uuid>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT field_name, old_value, new_value, changed_by, reason \
             FROM pricing_audit_logs WHERE version = $1 ORDER BY changed_at DESC LIMIT 1",
    )
    .bind(2)
    .fetch_one(&pool)
    .await
    .expect("audit log not found");

    assert_eq!(audit_row.0, "version");
    assert_eq!(audit_row.1, Some("1".to_string()));
    assert_eq!(audit_row.2, Some("2".to_string()));
    assert_eq!(audit_row.3, Some(user_id.0));
    assert_eq!(audit_row.4, reason);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pricing_audit_logs WHERE version = 2")
            .fetch_one(&pool)
            .await
            .expect("count failed");
    assert!(count >= 1);
}
