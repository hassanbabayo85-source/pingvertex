//! Regression tests for the scheduler-lease lifecycle invariant:
//!
//! > A claimed monitor execution must remain the sole owner of its
//! > monitor row for the entire duration of the external check. The
//! > lease may not expire while the check is still running, and it
//! > may not be resurrected after being legitimately recovered.
//!
//! These tests exercise `SchedulingRepository::renew_lease` and
//! `SchedulingRepository::recover_expired_leases` against a live
//! Postgres. The PAYG-era version of this file also verified that
//! wallet reservations stayed renewed alongside the lease; that half
//! was removed with the billing surface in phases 1 and 3.

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
async fn renew_lease_extends_expiry_for_current_owner() {
    // "execution shorter than lease": the current owner can safely renew
    // before the original expiry.
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    let monitor_id = insert_due_monitor(&pool, "renew-owner", user_id, tenant_id.0).await;

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let owner = Uuid::new_v4();
    let initial = repo
        .claim_due_monitors(1, owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(initial.len(), 1);

    let before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT lease_expires_at FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let extended = repo
        .renew_lease(
            MonitorId(monitor_id),
            owner,
            Duration::from_secs(120),
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    assert!(extended, "the current lease owner must be able to renew");

    let after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT lease_expires_at FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        after > before + chrono::Duration::seconds(60),
        "renewal must extend the lease by roughly the requested duration"
    );
}

#[tokio::test]
#[serial]
async fn renew_lease_rejects_stale_owner() {
    // "no duplicate claim": only the current owner may extend the lease.
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    let monitor_id = insert_due_monitor(&pool, "stale-owner-renew", user_id, tenant_id.0).await;

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let real_owner = Uuid::new_v4();
    let stale_owner = Uuid::new_v4();
    repo.claim_due_monitors(1, real_owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();

    let extended = repo
        .renew_lease(
            MonitorId(monitor_id),
            stale_owner,
            Duration::from_secs(120),
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    assert!(!extended, "a non-owner must not be able to renew the lease");

    let stored_owner: Option<Uuid> =
        sqlx::query_scalar("SELECT lease_owner FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_owner, Some(real_owner));
}

#[tokio::test]
#[serial]
async fn renew_lease_rejects_already_recovered_lease() {
    // "worker crash and lease recovery": once a lease has been recovered,
    // a late heartbeat from the dead worker must not resurrect it.
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    let monitor_id = insert_due_monitor(&pool, "recovered-renew", user_id, tenant_id.0).await;

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let owner = Uuid::new_v4();
    repo.claim_due_monitors(1, owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();

    // Simulate lease expiry + recovery (worker crash).
    sqlx::query("UPDATE monitors SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(monitor_id)
        .execute(&pool)
        .await
        .unwrap();
    repo.recover_expired_leases(chrono::Utc::now())
        .await
        .unwrap();

    // A late heartbeat from the dead worker must be rejected.
    let extended = repo
        .renew_lease(
            MonitorId(monitor_id),
            owner,
            Duration::from_secs(120),
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    assert!(
        !extended,
        "a recovered lease must not be resurrected by the previous owner"
    );

    let stored: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT lease_expires_at FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        stored.is_none(),
        "recovery must have cleared lease_expires_at"
    );
}

#[tokio::test]
#[serial]
async fn renew_lease_keeps_a_long_running_check_owned() {
    // "execution longer than the original lease": as long as heartbeats
    // arrive faster than the lease expires, the monitor remains exclusively
    // owned and cannot be reclaimed by another worker.
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    let monitor_id = insert_due_monitor(&pool, "long-check", user_id, tenant_id.0).await;

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let owner = Uuid::new_v4();
    let other_owner = Uuid::new_v4();

    // Claim with a short lease, shorter than the simulated check runtime.
    repo.claim_due_monitors(1, owner, Duration::from_secs(1), chrono::Utc::now())
        .await
        .unwrap();

    // Simulate the heartbeat loop of a long-running check: renew several
    // times, each renewal extending the lease past the initial expiry.
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let extended = repo
            .renew_lease(
                MonitorId(monitor_id),
                owner,
                Duration::from_secs(2),
                chrono::Utc::now(),
            )
            .await
            .unwrap();
        assert!(extended, "live owner must keep extending the lease");
    }

    // A second worker must not be able to claim the still-leased monitor,
    // even though the original 1-second lease would have expired by now.
    let stolen = repo
        .claim_due_monitors(1, other_owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    assert!(
        stolen.is_empty(),
        "a heartbeated lease must remain exclusively owned by its worker"
    );
}
