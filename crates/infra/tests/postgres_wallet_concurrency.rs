//! PostgreSQL concurrency tests for `PostgresWalletRepository`.
//!
//! These tests exercise the real row-level locking (`SELECT ... FOR UPDATE`)
//! and idempotency-key uniqueness that the wallet relies on for correctness
//! under concurrent load. They are the PostgreSQL counterpart to the
//! in-memory tests in `wallet_payg.rs`, which cannot exercise real
//! concurrent transactions.
//!
//! Coverage:
//!
//!   1. Concurrent `credit()` calls with distinct idempotency keys must all
//!      apply and accumulate.
//!   2. Concurrent `credit()` calls with the *same* idempotency key must
//!      result in exactly one credit; every caller must observe the same
//!      `WalletTransaction` row.
//!   3. Concurrent `reserve()` calls that together exceed the wallet
//!      balance must produce exactly one success and the rest must fail
//!      with "insufficient wallet balance" — no overspend.
//!   4. Concurrent `reserve()` calls with the same `request_id` must all
//!      return the same reservation (exactly one had `created=true`), and a
//!      subsequent double `capture()` must debit the wallet exactly once.
//!
//! Isolation strategy: every test allocates a fresh tenant (and its own
//! user row). Nothing is shared between tests, and nothing is cleaned up
//! afterwards — a fresh tenant per run is cheap and avoids racing other
//! tests that may run in parallel against the same database.
//!
//! Graceful skip: if `DATABASE_URL` is not set or the test Postgres is
//! unreachable, each test prints a SKIP line and returns Ok.

use application::ports::{TenantRepository, WalletRepository};
use chrono::{Duration, Utc};
use domain::WalletTransactionType;
use domain::shared::ids::{TenantId, UserId};
use infra::repositories::postgres_tenant_repository::PostgresTenantRepository;
use infra::repositories::postgres_wallet_repository::PostgresWalletRepository;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::task::JoinSet;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn load_env_test() {
    for candidate in &["../../.env.test", ".env.test", "../.env.test"] {
        if std::path::Path::new(candidate).exists() {
            dotenvy::from_filename(candidate).ok();
            return;
        }
    }
}

async fn pool_or_skip() -> Option<(sqlx::PgPool, infra::test_support::ExclusiveTestGuard)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    Some(infra::test_support::setup_pool_exclusive(&url).await)
}

/// Creates a fresh user + tenant + wallet, optionally seeded with an
/// initial balance. Mirrors `setup_tenant_and_wallet` in
/// `crates/worker/tests/payg_part3_auto_topup_load.rs`.
async fn setup_wallet(
    pool: &PgPool,
    initial_balance: i64,
) -> (Uuid, Arc<PostgresWalletRepository>) {
    let user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
         VALUES ($1, $2, 'not-a-real-hash', 'Wallet Concurrency Test', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@wallet-concurrency.test", user_id.0))
    .execute(pool)
    .await
    .expect("insert test user");

    let tenant_repo = PostgresTenantRepository::new(pool.clone());
    let tenant = domain::Tenant::new(
        TenantId::new(),
        format!("Wallet Concurrency Test {}", user_id.0),
        user_id,
    )
    .expect("construct tenant");
    tenant_repo.create(&tenant).await.expect("create tenant");

    let wallet_repo = Arc::new(PostgresWalletRepository::new(pool.clone()));
    wallet_repo
        .get_or_create(tenant.id.0, "USD")
        .await
        .expect("create wallet");

    if initial_balance > 0 {
        wallet_repo
            .credit(
                tenant.id.0,
                initial_balance,
                WalletTransactionType::TopUp,
                &format!("seed-{}", Uuid::new_v4()),
                Some("seed"),
            )
            .await
            .expect("seed balance");
    }

    (tenant.id.0, wallet_repo)
}

#[tokio::test]
async fn concurrent_credits_with_distinct_keys_all_apply() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_wallet(&pool, 0).await;

    const TASKS: i64 = 10;
    const PER_TASK: i64 = 100;

    let mut tasks = JoinSet::new();
    for i in 0..TASKS {
        let wallet = wallet.clone();
        tasks.spawn(async move {
            wallet
                .credit(
                    tenant_id,
                    PER_TASK,
                    WalletTransactionType::TopUp,
                    &format!("concurrent-distinct-{i}-{}", Uuid::new_v4()),
                    None,
                )
                .await
        });
    }

    let mut successes = 0;
    while let Some(res) = tasks.join_next().await {
        match res.expect("task must not panic") {
            Ok(_) => successes += 1,
            Err(e) => panic!("credit must not fail: {e}"),
        }
    }
    assert_eq!(successes, TASKS, "all distinct-key credits must succeed");

    let final_wallet = wallet
        .find(tenant_id)
        .await
        .expect("find wallet")
        .expect("wallet must exist");
    assert_eq!(
        final_wallet.balance_units,
        TASKS * PER_TASK,
        "balance must reflect every distinct-key credit"
    );
}

// ===========================================================================
// Test 2 — concurrent credits with the SAME idempotency key
// ===========================================================================

#[tokio::test]
async fn concurrent_credits_with_same_idempotency_key_apply_once() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_wallet(&pool, 0).await;

    const TASKS: usize = 10;
    const PER_TASK: i64 = 100;
    // A single shared idempotency key for the whole run.
    let shared_key = format!("concurrent-same-{}", Uuid::new_v4());

    let mut tasks = JoinSet::new();
    for _ in 0..TASKS {
        let wallet = wallet.clone();
        let key = shared_key.clone();
        tasks.spawn(async move {
            wallet
                .credit(
                    tenant_id,
                    PER_TASK,
                    WalletTransactionType::TopUp,
                    &key,
                    None,
                )
                .await
        });
    }

    let mut tx_ids = std::collections::HashSet::new();
    while let Some(res) = tasks.join_next().await {
        let tx = res
            .expect("task must not panic")
            .expect("credit must not fail");
        tx_ids.insert(tx.id);
    }
    assert_eq!(
        tx_ids.len(),
        1,
        "all concurrent credits with the same idempotency key must return the same WalletTransaction"
    );

    let final_wallet = wallet
        .find(tenant_id)
        .await
        .expect("find wallet")
        .expect("wallet must exist");
    assert_eq!(
        final_wallet.balance_units, PER_TASK,
        "balance must reflect exactly one credit for a shared idempotency key"
    );
}

// ===========================================================================
// Test 3 — concurrent reservations cannot overspend
// ===========================================================================

#[tokio::test]
async fn concurrent_reservations_cannot_overspend() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    // Balance covers exactly one reservation of the same amount.
    let (tenant_id, wallet) = setup_wallet(&pool, 100).await;

    const TASKS: usize = 5;
    const PER_RESERVATION: i64 = 100;

    let mut tasks = JoinSet::new();
    for i in 0..TASKS {
        let wallet = wallet.clone();
        tasks.spawn(async move {
            wallet
                .reserve(
                    tenant_id,
                    PER_RESERVATION,
                    "http_check.monitoring",
                    1,
                    1,
                    "USD",
                    &format!("concurrent-reserve-{i}-{}", Uuid::new_v4()),
                    Utc::now() + Duration::seconds(60),
                )
                .await
        });
    }

    let mut successes = 0usize;
    let mut failures = 0usize;
    while let Some(res) = tasks.join_next().await {
        match res.expect("task must not panic") {
            Ok(_) => successes += 1,
            Err(e) => {
                assert!(
                    e.to_string().contains("insufficient wallet balance"),
                    "unexpected error: {e}"
                );
                failures += 1;
            }
        }
    }

    assert_eq!(
        successes, 1,
        "exactly one concurrent reservation must succeed"
    );
    assert_eq!(
        failures,
        TASKS - 1,
        "every other reservation must be rejected for insufficient balance"
    );
}

// ===========================================================================
// Test 4 — same request_id is idempotent, and capture debits at most once
// ===========================================================================

#[tokio::test]
async fn same_request_id_concurrent_reservation_and_capture_is_idempotent() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_wallet(&pool, 1_000).await;

    let shared_request_id = format!("concurrent-idem-{}", Uuid::new_v4());

    let mut tasks = JoinSet::new();
    for _ in 0..2 {
        let wallet = wallet.clone();
        let req = shared_request_id.clone();
        tasks.spawn(async move {
            wallet
                .reserve(
                    tenant_id,
                    100,
                    "http_check.monitoring",
                    1,
                    1,
                    "USD",
                    &req,
                    Utc::now() + Duration::seconds(60),
                )
                .await
        });
    }

    let mut outcomes = Vec::new();
    while let Some(res) = tasks.join_next().await {
        outcomes.push(
            res.expect("task must not panic")
                .expect("reserve must not fail"),
        );
    }
    assert_eq!(outcomes.len(), 2);

    let reservation_id = outcomes[0].reservation.id;
    assert_eq!(
        outcomes[1].reservation.id, reservation_id,
        "both concurrent reservations must share the same row"
    );

    let created_count = outcomes.iter().filter(|o| o.created).count();
    assert_eq!(
        created_count, 1,
        "exactly one of the concurrent reservations must report created=true"
    );

    // Two sequential captures on the same reservation must be idempotent:
    // both must return the same transaction and debit the wallet once.
    let tx1 = wallet
        .capture(reservation_id, None, &[], None)
        .await
        .expect("first capture");
    let tx2 = wallet
        .capture(reservation_id, None, &[], None)
        .await
        .expect("second capture");
    assert_eq!(
        tx1.id, tx2.id,
        "a second capture of the same reservation must return the original transaction"
    );
    assert_eq!(
        tx1.balance_after_units, tx2.balance_after_units,
        "balance_after must be identical across replays"
    );

    let final_wallet = wallet
        .find(tenant_id)
        .await
        .expect("find wallet")
        .expect("wallet must exist");
    assert_eq!(
        final_wallet.balance_units, 900,
        "only 100 units must be debited even with two concurrent reserves and two captures"
    );
}
