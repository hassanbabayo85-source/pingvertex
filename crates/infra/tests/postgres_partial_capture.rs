//! GAP 3 Kashi B: partial-capture tests for `PostgresWalletRepository`.
//!
//! When `capture(..., actual_charge_units: Some(n))` is called with
//! `0 < n <= reservation.amount_units`, the wallet must be debited by
//! exactly `n` and the reservation must be marked CAPTURED — the
//! remainder of the reservation is released in the same transaction.
//!
//! Coverage:
//!
//!   1. `capture_full_amount_when_actual_is_none` — backward compatibility.
//!   2. `capture_partial_amount_releases_difference` — the core feature.
//!   3. `capture_rejects_actual_greater_than_reserved` — boundary.
//!   4. `capture_rejects_zero_or_negative_actual` — boundary.
//!   5. `partial_capture_is_idempotent` — replay safety.
//!
//! Graceful skip: if `DATABASE_URL` is unset or Postgres is unreachable,
//! each test prints nothing and returns early (identical to
//! `postgres_wallet_concurrency.rs`).

use application::ports::{TenantRepository, WalletRepository};
use chrono::{Duration, Utc};
use domain::WalletTransactionType;
use domain::shared::ids::{TenantId, UserId};
use infra::repositories::postgres_tenant_repository::PostgresTenantRepository;
use infra::repositories::postgres_wallet_repository::PostgresWalletRepository;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[allow(dead_code)]
fn load_env_test() {
    for candidate in &["../../.env.test", ".env.test", "../.env.test"] {
        if std::path::Path::new(candidate).exists() {
            dotenvy::from_filename(candidate).ok();
            return;
        }
    }
}

async fn pool_or_skip() -> Option<(PgPool, infra::test_support::ExclusiveTestGuard)> {
    load_env_test();
    let url = std::env::var("DATABASE_URL").ok()?;
    Some(infra::test_support::setup_pool_exclusive(&url).await)
}

async fn setup_wallet(
    pool: &PgPool,
    initial_balance: i64,
) -> (Uuid, Arc<PostgresWalletRepository>) {
    let user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
         VALUES ($1, $2, 'not-a-real-hash', 'Partial Capture Test', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@partial-capture.test", user_id.0))
    .execute(pool)
    .await
    .expect("insert test user");

    let tenant_repo = PostgresTenantRepository::new(pool.clone());
    let tenant = domain::Tenant::new(TenantId::new(), "Partial Capture Tenant", user_id).unwrap();
    tenant_repo.create(&tenant).await.unwrap();

    let wallet = Arc::new(PostgresWalletRepository::new(pool.clone()));
    wallet.get_or_create(tenant.id.0, "USD").await.unwrap();
    if initial_balance > 0 {
        wallet
            .credit(
                tenant.id.0,
                initial_balance,
                WalletTransactionType::TopUp,
                &format!("seed-{}", Uuid::new_v4()),
                None,
            )
            .await
            .unwrap();
    }
    (tenant.id.0, wallet)
}

fn unique_request_id() -> String {
    format!("partial-capture-{}", Uuid::new_v4())
}

#[tokio::test]
async fn capture_full_amount_when_actual_is_none() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant, wallet) = setup_wallet(&pool, 100_000).await;

    let outcome = wallet
        .reserve(
            tenant,
            10_000,
            "test_op",
            1_000,
            1,
            "USD",
            &unique_request_id(),
            Utc::now() + Duration::seconds(60),
        )
        .await
        .unwrap();

    let tx = wallet
        .capture(outcome.reservation.id, None, &[], None)
        .await
        .unwrap();

    assert_eq!(tx.amount_units, -10_000);
    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(w.balance_units, 90_000);
}

#[tokio::test]
async fn capture_partial_amount_releases_difference() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant, wallet) = setup_wallet(&pool, 100_000).await;

    // Reserve 10_000, capture only 3_000 (e.g. a flow that ran 3 of 10 steps).
    let outcome = wallet
        .reserve(
            tenant,
            10_000,
            "api_flow_check.monitoring",
            1_000,
            1,
            "USD",
            &unique_request_id(),
            Utc::now() + Duration::seconds(60),
        )
        .await
        .unwrap();

    let tx = wallet
        .capture(outcome.reservation.id, None, &[], Some(3_000))
        .await
        .unwrap();

    assert_eq!(
        tx.amount_units, -3_000,
        "partial capture debits only actual"
    );
    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(
        w.balance_units, 97_000,
        "7_000 of the reservation must be released"
    );

    // The reservation is terminal (CAPTURED), not left PENDING.
    let again = wallet
        .capture(outcome.reservation.id, None, &[], Some(3_000))
        .await
        .unwrap();
    assert_eq!(again.amount_units, -3_000, "replay returns original debit");
    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(w.balance_units, 97_000, "replay must not double-debit");
}

#[tokio::test]
async fn capture_rejects_actual_greater_than_reserved() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant, wallet) = setup_wallet(&pool, 100_000).await;

    let outcome = wallet
        .reserve(
            tenant,
            10_000,
            "test_op",
            1_000,
            1,
            "USD",
            &unique_request_id(),
            Utc::now() + Duration::seconds(60),
        )
        .await
        .unwrap();

    let err = wallet
        .capture(outcome.reservation.id, None, &[], Some(20_000))
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("actual_charge_units"),
        "expected boundary error, got: {err}"
    );

    // Wallet must be untouched — the failed capture must not debit.
    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(w.balance_units, 100_000);
}

#[tokio::test]
async fn capture_rejects_zero_or_negative_actual() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant, wallet) = setup_wallet(&pool, 100_000).await;

    for bad in [0_i64, -5_i64] {
        let outcome = wallet
            .reserve(
                tenant,
                10_000,
                "test_op",
                1_000,
                1,
                "USD",
                &unique_request_id(),
                Utc::now() + Duration::seconds(60),
            )
            .await
            .unwrap();

        let err = wallet
            .capture(outcome.reservation.id, None, &[], Some(bad))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("actual_charge_units"),
            "expected boundary error for {bad}, got: {err}"
        );
    }

    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(w.balance_units, 100_000);
}

#[tokio::test]
async fn partial_capture_is_idempotent() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant, wallet) = setup_wallet(&pool, 100_000).await;

    let outcome = wallet
        .reserve(
            tenant,
            10_000,
            "api_flow_check.monitoring",
            1_000,
            1,
            "USD",
            &unique_request_id(),
            Utc::now() + Duration::seconds(60),
        )
        .await
        .unwrap();

    let first = wallet
        .capture(outcome.reservation.id, None, &[], Some(4_000))
        .await
        .unwrap();

    // A concurrent worker may re-issue the same capture. It must observe
    // the same transaction row and must not touch the balance again.
    let second = wallet
        .capture(outcome.reservation.id, None, &[], Some(4_000))
        .await
        .unwrap();

    assert_eq!(first.id, second.id, "replay returns the same ledger row");

    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(
        w.balance_units, 96_000,
        "exactly one 4_000 debit despite two capture calls"
    );
}
