use std::{sync::Arc, time::Duration};

use application::ports::{AgentLeaseRepository, SchedulingRepository, WalletRepository};
use domain::shared::ids::MonitorId;
use infra::{PostgresAgentLeaseRepository, PostgresSchedulingRepository};
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

async fn insert_due_monitor_with_interval(
    pool: &PgPool,
    name: &str,
    user_id: Uuid,
    tenant_id: Uuid,
    interval_seconds: i32,
) -> Uuid {
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
            200, $5, 10,
            true, true, 14, 'http',
            now(), now(), now() - interval '1 minute'
        )
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(tenant_id)
    .bind(name)
    .bind(interval_seconds)
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
    let monitor_id =
        insert_due_monitor_with_interval(&pool, "crash-recovery", user_id, tenant_id.0, 300).await;

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
    let first_execution_id = claimed[0].execution_id;

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
    assert_eq!(claimed_again[0].execution_id, first_execution_id);

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
    let monitor_id =
        insert_due_monitor_with_interval(&pool, "stale-owner", user_id, tenant_id.0, 300).await;

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

#[tokio::test]
#[serial]
async fn free_entitlement_is_not_restored_after_payg_activation_and_exhaustion() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;

    let monitor_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO monitors (
            id, user_id, tenant_id, regions, name, url, method,
            expected_status, interval_seconds, timeout_seconds,
            enabled, ssl_check_enabled, ssl_expiry_warning_days, kind,
            created_at, updated_at, next_due_at
        ) VALUES (
            $1, $2, $3, ARRAY['us-east'], 'free-eligible', 'http://example.com', 'GET',
            200, 300, 10, true, true, 14, 'http', now(), now(), now()
        )
        "#,
    )
    .bind(monitor_id)
    .bind(user_id)
    .bind(tenant_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let owner = Uuid::new_v4();
    let free_claim = repo
        .claim_due_monitors(1, owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(free_claim.len(), 1);
    assert_eq!(free_claim[0].monitor.tenant_id, tenant_id);
    repo.complete_claim(MonitorId(monitor_id), owner)
        .await
        .unwrap();

    let wallet = infra::PostgresWalletRepository::new(pool.clone());
    wallet.get_or_create(tenant_id.0, "USD").await.unwrap();
    wallet
        .credit(
            tenant_id.0,
            2,
            domain::WalletTransactionType::TopUp,
            "scheduler-free-fallback-topup",
            None,
        )
        .await
        .unwrap();
    let reservation = wallet
        .reserve(
            tenant_id.0,
            2,
            "http_check.monitoring",
            100,
            1,
            "USD",
            "scheduler-free-fallback-charge",
            chrono::Utc::now() + chrono::Duration::minutes(1),
        )
        .await
        .unwrap();
    wallet
        .capture(reservation.reservation.id, None, &[])
        .await
        .unwrap();

    sqlx::query("UPDATE monitors SET next_due_at=now() WHERE id=$1")
        .bind(monitor_id)
        .execute(&pool)
        .await
        .unwrap();

    let owner2 = Uuid::new_v4();
    let paid_claim = repo
        .claim_due_monitors(1, owner2, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(paid_claim.len(), 1);
    assert_eq!(paid_claim[0].monitor.tenant_id, tenant_id);
}

#[tokio::test]
#[serial]
async fn per_tenant_claim_limit_is_enforced() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;

    for index in 0..10 {
        sqlx::query(
            r#"
            INSERT INTO monitors (
                id, user_id, tenant_id, regions, name, url, method,
                expected_status, interval_seconds, timeout_seconds,
                enabled, ssl_check_enabled, ssl_expiry_warning_days, kind,
                created_at, updated_at, next_due_at
            ) VALUES (
                $1, $2, $3, ARRAY['us-east'], $4, $5, 'GET',
                200, 300, 10, true, true, 14, 'http',
                now(), now(), now()
            )
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(tenant_id.0)
        .bind(format!("cap-{index}"))
        .bind(format!("https://cap-{index}.example.com"))
        .execute(&pool)
        .await
        .unwrap();
    }

    let repo = PostgresSchedulingRepository::new(pool);
    let claims = repo
        .claim_due_monitors_with_limits(
            10,
            Uuid::new_v4(),
            Duration::from_secs(60),
            chrono::Utc::now(),
            2,
            2,
        )
        .await
        .unwrap();

    assert_eq!(claims.len(), 2);
}

#[tokio::test]
#[serial]
async fn ten_thousand_due_monitors_are_capped_per_tenant() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;

    sqlx::query(
        r#"
        INSERT INTO monitors (
            id, user_id, tenant_id, regions, name, url, method,
            expected_status, interval_seconds, timeout_seconds,
            enabled, ssl_check_enabled, ssl_expiry_warning_days, kind,
            created_at, updated_at, next_due_at
        )
        SELECT
            gen_random_uuid(), $1, $2, ARRAY['us-east'],
            'herd-' || g::text,
            'https://herd-' || g::text || '.example.com',
            'GET', 200, 300, 10, true, true, 14, 'http',
            now(), now(), now()
        FROM generate_series(1, 10000) AS g
        "#,
    )
    .bind(user_id)
    .bind(tenant_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let repo = PostgresSchedulingRepository::new(pool);
    let claims = repo
        .claim_due_monitors_with_limits(
            100,
            Uuid::new_v4(),
            Duration::from_secs(60),
            chrono::Utc::now(),
            4,
            4,
        )
        .await
        .unwrap();

    assert_eq!(claims.len(), 4);
}

#[tokio::test]
#[serial]
async fn agent_region_claim_respects_cross_region_tenant_cap() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;

    for index in 0..10 {
        let monitor_id =
            insert_due_monitor(&pool, &format!("agent-cap-{index}"), user_id, tenant_id.0).await;
        sqlx::query("UPDATE monitors SET regions = ARRAY['us-east','eu-west'] WHERE id = $1")
            .bind(monitor_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    let repo = PostgresAgentLeaseRepository::new(pool);
    let claims = repo
        .claim_due_checks_with_limits(
            "agent-1",
            &domain::RegionCode::new("us-east").unwrap(),
            100,
            Duration::from_secs(120),
            chrono::Utc::now(),
            2,
            4,
        )
        .await
        .unwrap();

    assert_eq!(claims.len(), 2);
}

/// Release-gate load test: exercises the real PostgreSQL claim query at the
/// 10k-due-monitor boundary. It is ignored during the ordinary unit/integration
/// suite because it intentionally creates a large fixture; run it explicitly
/// with `cargo test -p infra --test scheduler_claim ten_thousand_due_monitors -- --ignored --nocapture`.
#[tokio::test]
#[serial]
#[ignore]
async fn ten_thousand_due_monitors_can_be_claimed_without_duplicates() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    sqlx::query(
        r#"
        INSERT INTO monitors (
            id, user_id, tenant_id, regions, name, url, method,
            expected_status, interval_seconds, timeout_seconds,
            enabled, ssl_check_enabled, ssl_expiry_warning_days, kind,
            created_at, updated_at, next_due_at
        )
        SELECT gen_random_uuid(), $1, $2, ARRAY['us-east'], 'load-' || g,
               'http://example.com', 'GET', 200, 60, 10, true, true, 14,
               'http', now(), now(), now() - interval '1 minute'
        FROM generate_series(1, 10000) AS g
        "#,
    )
    .bind(user_id)
    .bind(tenant_id.0)
    .execute(&pool)
    .await
    .unwrap();

    let repo = Arc::new(PostgresSchedulingRepository::new(pool.clone()));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..16 {
        let repo = repo.clone();
        tasks.spawn(async move {
            repo.claim_due_monitors(
                625,
                Uuid::new_v4(),
                Duration::from_secs(120),
                chrono::Utc::now(),
            )
            .await
        });
    }

    let mut ids = Vec::with_capacity(10_000);
    while let Some(result) = tasks.join_next().await {
        ids.extend(result.unwrap().unwrap().into_iter().map(|m| m.id.0));
    }
    ids.sort_unstable();
    let claimed = ids.len();
    ids.dedup();
    assert_eq!(claimed, 10_000);
    assert_eq!(ids.len(), 10_000, "no monitor may be claimed twice");

    let leased: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM monitors WHERE tenant_id=$1 AND lease_owner IS NOT NULL",
    )
    .bind(tenant_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(leased, 10_000);
}
