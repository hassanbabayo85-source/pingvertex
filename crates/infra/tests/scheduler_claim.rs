use std::time::Duration;

use application::ports::SchedulingRepository;
use domain::shared::ids::MonitorId;
use infra::PostgresSchedulingRepository;
use serial_test::serial;
use sqlx::PgPool;
use uuid::Uuid;

mod common;

async fn insert_due_monitor(pool: &PgPool, name: &str, user_id: Uuid, tenant_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO monitors (
            id, user_id, tenant_id, regions, name, url, method,
            expected_status, interval_seconds, timeout_seconds,
            enabled, ssl_check_enabled, ssl_expiry_warning_days, kind,
            created_at, updated_at, next_due_at
        )
        VALUES (
            $1, $2, $3, ARRAY['us-east'], $4, 'http://example.com', 'GET',
            200, 60, 10,
            true, true, 14, 'http',
            now(), now(), now() - interval '1 minute'
        )
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(tenant_id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    id
}

#[tokio::test]
#[serial]
async fn concurrent_claim_attempts_do_not_double_claim() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;

    let mut ids = Vec::new();
    for i in 0..20 {
        ids.push(insert_due_monitor(&pool, &format!("monitor-{i}"), user_id, tenant_id.0).await);
    }

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let lease_duration = Duration::from_secs(60);
    let now = chrono::Utc::now();

    let (res1, res2) = tokio::join!(
        repo.claim_due_monitors(10, Uuid::new_v4(), lease_duration, now),
        repo.claim_due_monitors(10, Uuid::new_v4(), lease_duration, now)
    );

    let mut claimed_ids = Vec::new();
    for monitor in res1.unwrap().into_iter().chain(res2.unwrap()) {
        claimed_ids.push(monitor.id.0);
    }

    // Concurrent callers must never double-claim a monitor.
    //
    // The two calls may race such that one caller returns fewer rows,
    // but the database state must still contain all successfully claimed
    // monitors and no monitor may be returned twice.
    assert!(
        claimed_ids.len() <= 20,
        "concurrent claimers must not claim more than the available monitors"
    );

    let claimed_count = claimed_ids.len();

    claimed_ids.sort();
    claimed_ids.dedup();

    assert_eq!(
        claimed_ids.len(),
        claimed_count,
        "concurrent claimers must never return the same monitor twice"
    );

    let leased_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM monitors WHERE lease_owner IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(
        leased_count, 20,
        "all 20 due monitors must be leased after the concurrent claims"
    );
}

#[tokio::test]
#[serial]
async fn expired_lease_makes_monitor_claimable_again() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    let monitor_id = insert_due_monitor(&pool, "crash-recovery", user_id, tenant_id.0).await;

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let lease_duration = Duration::from_secs(60);

    // First claim succeeds.
    let first_owner = Uuid::new_v4();
    let claimed = repo
        .claim_due_monitors(1, first_owner, lease_duration, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id.0, monitor_id);

    // Simulate crash: manually force lease expiry.
    sqlx::query("UPDATE monitors SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(monitor_id)
        .execute(&pool)
        .await
        .unwrap();

    // Recovery clears expired lease.
    let recovered = repo
        .recover_expired_leases(chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(recovered, 1);

    // Second claim succeeds after recovery.
    let second_owner = Uuid::new_v4();
    let claimed_again = repo
        .claim_due_monitors(1, second_owner, lease_duration, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(claimed_again.len(), 1);
    assert_eq!(claimed_again[0].id.0, monitor_id);

    // Complete claim clears lease and advances next_due_at.
    repo.complete_claim(MonitorId(monitor_id)).await.unwrap();
    let lease_owner: Option<Uuid> =
        sqlx::query_scalar("SELECT lease_owner FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(lease_owner.is_none());

    let next_due_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_due_at FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(next_due_at > chrono::Utc::now());
}
