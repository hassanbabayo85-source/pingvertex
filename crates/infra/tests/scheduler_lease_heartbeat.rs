//! Regression tests for the scheduler-lease vs monitor-execution lifecycle
//! invariant:
//!
//! > A claimed monitor execution must remain the sole owner of its monitor
//! > row and its PAYG reservation for the entire duration of the external
//! > check. Neither the lease nor the reservation may expire while the
//! > check is still running, and neither may be resurrected after being
//! > legitimately recovered.
//!
//! These tests exercise the primitives the durable scheduler heartbeats:
//! `SchedulingRepository::renew_lease` and
//! `WalletRepository::renew_reservation_by_request`, plus the interaction
//! between renewal and `WalletRepository::expire_pending_reservations`.

use std::time::Duration;

use application::ports::{SchedulingRepository, WalletRepository};
use domain::shared::ids::MonitorId;
use infra::{PostgresSchedulingRepository, PostgresWalletRepository};
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

async fn seed_wallet(pool: &PgPool, tenant_id: Uuid, currency: &str) {
    let wallet = PostgresWalletRepository::new(pool.clone());
    wallet.get_or_create(tenant_id, currency).await.unwrap();
    wallet
        .credit(
            tenant_id,
            100_000,
            domain::WalletTransactionType::TopUp,
            &format!("test-topup-{}", Uuid::new_v4()),
            None,
        )
        .await
        .unwrap();
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

#[tokio::test]
#[serial]
async fn renew_reservation_by_request_extends_pending() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    seed_wallet(&pool, tenant_id.0, "USD").await;

    let wallet = PostgresWalletRepository::new(pool.clone());
    let request_id = format!("test-request-{}", Uuid::new_v4());
    let initial_expires = chrono::Utc::now() + chrono::Duration::seconds(30);
    let outcome = wallet
        .reserve(
            tenant_id.0,
            10,
            "http_check.monitoring",
            1,
            1,
            "USD",
            &request_id,
            initial_expires,
        )
        .await
        .unwrap();
    assert!(outcome.created);

    let extended_expires = chrono::Utc::now() + chrono::Duration::seconds(300);
    let extended = wallet
        .renew_reservation_by_request(tenant_id.0, &request_id, extended_expires)
        .await
        .unwrap();
    assert!(extended, "a PENDING reservation must be renewable");

    let stored = wallet
        .find_reservation_by_request(tenant_id.0, &request_id)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.expires_at > initial_expires);
    assert_eq!(stored.status, domain::CreditReservation::PENDING);
}

#[tokio::test]
#[serial]
async fn renew_reservation_by_request_rejects_captured() {
    // "no duplicate charge": once the reservation has been captured, a late
    // heartbeat must not resurrect or extend it.
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    seed_wallet(&pool, tenant_id.0, "USD").await;

    let wallet = PostgresWalletRepository::new(pool.clone());
    let request_id = format!("test-request-{}", Uuid::new_v4());
    let outcome = wallet
        .reserve(
            tenant_id.0,
            10,
            "http_check.monitoring",
            1,
            1,
            "USD",
            &request_id,
            chrono::Utc::now() + chrono::Duration::seconds(60),
        )
        .await
        .unwrap();
    wallet
        .capture(outcome.reservation.id, None, &[])
        .await
        .unwrap();

    let extended = wallet
        .renew_reservation_by_request(
            tenant_id.0,
            &request_id,
            chrono::Utc::now() + chrono::Duration::seconds(600),
        )
        .await
        .unwrap();
    assert!(!extended, "a captured reservation must not be renewable");

    // Capture must still be idempotent (no duplicate charge).
    let second = wallet.capture(outcome.reservation.id, None, &[]).await;
    assert!(second.is_ok(), "capture must remain idempotent");
}

#[tokio::test]
#[serial]
async fn expire_worker_does_not_expire_heartbeated_reservation() {
    // "reservation cannot expire while valid execution is still active":
    // the expiration worker must not race against an in-flight heartbeat.
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    seed_wallet(&pool, tenant_id.0, "USD").await;

    let wallet = PostgresWalletRepository::new(pool.clone());
    let request_id = format!("test-request-{}", Uuid::new_v4());
    // Reservation that is already past its initial expiry.
    let outcome = wallet
        .reserve(
            tenant_id.0,
            10,
            "http_check.monitoring",
            1,
            1,
            "USD",
            &request_id,
            chrono::Utc::now() - chrono::Duration::seconds(1),
        )
        .await
        .unwrap();

    // A heartbeat for the live execution extends it *before* the expiry
    // worker runs. This mirrors the real ordering: heartbeats run every few
    // seconds, the expiration worker runs every few minutes.
    let extended = wallet
        .renew_reservation_by_request(
            tenant_id.0,
            &request_id,
            chrono::Utc::now() + chrono::Duration::seconds(300),
        )
        .await
        .unwrap();
    assert!(
        extended,
        "renewal must succeed even on an expired reservation"
    );

    // Now run the expiry worker.
    let _ = wallet.expire_pending_reservations(500).await.unwrap();

    let stored = wallet
        .find_reservation_by_request(tenant_id.0, &request_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.status,
        domain::CreditReservation::PENDING,
        "an active heartbeat must protect the reservation from the expiry worker"
    );

    // And the final capture must still succeed.
    wallet
        .capture(outcome.reservation.id, None, &[])
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn retry_after_temporary_failure_reuses_same_lease_and_reservation() {
    // "retry after temporary failure": a retried execution must reuse the
    // same scheduler lease and the same PAYG reservation, so a temporary
    // failure cannot produce a duplicate charge.
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    let monitor_id = insert_due_monitor(&pool, "retry-test", user_id, tenant_id.0).await;
    seed_wallet(&pool, tenant_id.0, "USD").await;

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let wallet = PostgresWalletRepository::new(pool.clone());
    let owner = Uuid::new_v4();
    let claimed = repo
        .claim_due_monitors(1, owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    let execution_id = claimed[0].execution_id;

    let request_id = format!("monitor-check:{}:{}", monitor_id, execution_id);
    // First attempt.
    let r1 = wallet
        .reserve(
            tenant_id.0,
            10,
            "http_check.monitoring",
            1,
            1,
            "USD",
            &request_id,
            chrono::Utc::now() + chrono::Duration::seconds(60),
        )
        .await
        .unwrap();
    assert!(r1.created);

    // Retry: same request_id must reuse the same reservation, not create a new one.
    let r2 = wallet
        .reserve(
            tenant_id.0,
            10,
            "http_check.monitoring",
            1,
            1,
            "USD",
            &request_id,
            chrono::Utc::now() + chrono::Duration::seconds(60),
        )
        .await
        .unwrap();
    assert!(!r2.created, "retry must reuse the original reservation");
    assert_eq!(r1.reservation.id, r2.reservation.id);

    // Capture must be idempotent.
    wallet.capture(r1.reservation.id, None, &[]).await.unwrap();
    let second_capture = wallet.capture(r1.reservation.id, None, &[]).await;
    assert!(second_capture.is_ok(), "capture must be idempotent");
}
