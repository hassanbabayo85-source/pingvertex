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
    let first_claimed_at = claimed[0].claimed_at;

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
    assert_eq!(claimed_again[0].claimed_at, first_claimed_at);

    // Complete claim clears lease and advances next_due_at.
    repo.complete_claim(MonitorId(monitor_id), second_owner)
        .await
        .unwrap();
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

#[tokio::test]
#[serial]
async fn immediate_check_request_is_tenant_scoped_and_preserves_leases() {
    let pool = common::setup_pool().await;
    let user_a = Uuid::new_v4();
    let tenant_a = common::insert_user(&pool, domain::UserId(user_a)).await;
    let user_b = Uuid::new_v4();
    let tenant_b = common::insert_user(&pool, domain::UserId(user_b)).await;

    let a1 = insert_due_monitor(&pool, "manual-a-1", user_a, tenant_a.0).await;
    let a2 = insert_due_monitor(&pool, "manual-a-2", user_a, tenant_a.0).await;
    let b1 = insert_due_monitor(&pool, "manual-b-1", user_b, tenant_b.0).await;

    // Keep tenant B's monitor in the future so the test can verify
    // that request_immediate_checks() does not affect another tenant.
    sqlx::query("UPDATE monitors SET next_due_at = now() + interval '10 minutes' WHERE id = $1")
        .bind(b1)
        .execute(&pool)
        .await
        .unwrap();

    let lease_owner = Uuid::new_v4();
    sqlx::query("UPDATE monitors SET lease_owner = $1, lease_expires_at = now() + interval '60 seconds', lease_claimed_at = now(), next_due_at = now() + interval '10 minutes' WHERE id = $2")
        .bind(lease_owner)
        .bind(a2)
        .execute(&pool)
        .await
        .unwrap();

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let queued = repo.request_immediate_checks(tenant_a).await.unwrap();
    assert_eq!(queued, 2);

    let a1_due: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_due_at FROM monitors WHERE id = $1")
            .bind(a1)
            .fetch_one(&pool)
            .await
            .unwrap();
    let a2_due: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_due_at FROM monitors WHERE id = $1")
            .bind(a2)
            .fetch_one(&pool)
            .await
            .unwrap();
    let b1_due: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_due_at FROM monitors WHERE id = $1")
            .bind(b1)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(a1_due <= chrono::Utc::now());
    assert!(a2_due <= chrono::Utc::now());
    assert!(b1_due > chrono::Utc::now());

    let preserved_owner: Option<Uuid> =
        sqlx::query_scalar("SELECT lease_owner FROM monitors WHERE id = $1")
            .bind(a2)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(preserved_owner, Some(lease_owner));
}

#[tokio::test]
#[serial]
async fn stale_worker_cannot_complete_a_reclaimed_monitor_claim() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    let monitor_id = insert_due_monitor(&pool, "stale-owner", user_id, tenant_id.0).await;

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let first_owner = Uuid::new_v4();
    let second_owner = Uuid::new_v4();
    let first = repo
        .claim_due_monitors(1, first_owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(first.len(), 1);

    sqlx::query("UPDATE monitors SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(monitor_id)
        .execute(&pool)
        .await
        .unwrap();
    repo.recover_expired_leases(chrono::Utc::now())
        .await
        .unwrap();

    let second = repo
        .claim_due_monitors(1, second_owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(second.len(), 1);

    let stale_result = repo
        .complete_claim(MonitorId(monitor_id), first_owner)
        .await;
    assert!(stale_result.is_err());

    let current_owner: Option<Uuid> =
        sqlx::query_scalar("SELECT lease_owner FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(current_owner, Some(second_owner));

    repo.complete_claim(MonitorId(monitor_id), second_owner)
        .await
        .unwrap();
}
