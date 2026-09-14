//! PART 4 — Financial concurrency tests.
//!
//! Five invariants that must hold under concurrent load:
//!
//!   1. double capture            — a captured reservation is debited once
//!   2. double top-up             — same idempotency key credits once
//!   3. duplicate webhook         — same provider event is processed once
//!   4. duplicate usage request   — same (tenant, request_id) inserts once
//!   5. concurrent reservation    — concurrent reserves cannot overspend
//!
//! Graceful skip: if `DATABASE_URL` is unset or Postgres is unreachable,
//! each test returns Ok without asserting anything.

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

async fn setup_tenant_wallet(
    pool: &PgPool,
    initial_balance: i64,
) -> (Uuid, Arc<PostgresWalletRepository>) {
    let user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
         VALUES ($1, $2, 'not-a-real-hash', 'PART4 Concurrency', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@part4-concurrency.test", user_id.0))
    .execute(pool)
    .await
    .expect("insert test user");

    let tenant_repo = PostgresTenantRepository::new(pool.clone());
    let tenant = domain::Tenant::new(TenantId::new(), "PART4 Concurrency Tenant", user_id).unwrap();
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

// ===========================================================================
// 1) double capture — concurrent capture of the same reservation debits once
// ===========================================================================

#[tokio::test]
async fn double_capture_debits_wallet_once() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 10_000).await;

    let outcome = wallet
        .reserve(
            tenant_id,
            1_000,
            "http_check.one_time",
            1_000,
            1,
            "USD",
            &format!("part4-cap-{}", Uuid::new_v4()),
            Utc::now() + Duration::minutes(15),
        )
        .await
        .unwrap();
    let reservation_id = outcome.reservation.id;

    let mut tasks = JoinSet::new();
    for _ in 0..4 {
        let w = wallet.clone();
        tasks.spawn(async move { w.capture(reservation_id, None, &[], None).await });
    }

    let mut successes = 0;
    let mut errors = 0;
    while let Some(res) = tasks.join_next().await {
        match res.unwrap() {
            Ok(_) => successes += 1,
            Err(_) => errors += 1,
        }
    }
    // All four must return the SAME WalletTransaction row (idempotent replay).
    assert!(successes + errors == 4);
    assert_eq!(errors, 0, "no error expected — replays return the same row");

    let debits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wallet_transactions
         WHERE tenant_id=$1 AND transaction_type='DEBIT' AND idempotency_key=$2",
    )
    .bind(tenant_id)
    .bind(format!("reservation:{reservation_id}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(debits, 1, "double capture must debit exactly once");

    let balance = wallet
        .get_or_create(tenant_id, "USD")
        .await
        .unwrap()
        .balance_units;
    assert_eq!(
        balance, 9_000,
        "wallet balance must reflect a single 1_000 debit"
    );
}

// ===========================================================================
// 2) double top-up — same idempotency key credits once
// ===========================================================================

#[tokio::test]
async fn double_top_up_credits_wallet_once() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 0).await;

    let key = format!("part4-topup-{}", Uuid::new_v4());

    let mut tasks = JoinSet::new();
    for _ in 0..4 {
        let w = wallet.clone();
        let k = key.clone();
        tasks.spawn(async move {
            w.credit(tenant_id, 5_000, WalletTransactionType::TopUp, &k, None)
                .await
        });
    }

    let mut ids = std::collections::HashSet::new();
    while let Some(res) = tasks.join_next().await {
        match res.unwrap() {
            Ok(tx) => {
                ids.insert(tx.id);
            }
            Err(e) => panic!("expected idempotent success, got error: {e}"),
        }
    }
    assert_eq!(
        ids.len(),
        1,
        "all four top-up calls must return the same transaction"
    );

    let balance = wallet
        .get_or_create(tenant_id, "USD")
        .await
        .unwrap()
        .balance_units;
    assert_eq!(balance, 5_000, "double top-up must credit exactly once");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wallet_transactions WHERE tenant_id=$1 AND idempotency_key=$2",
    )
    .bind(tenant_id)
    .bind(&key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "exactly one ledger row per idempotency key");
}

// ===========================================================================
// 3) duplicate webhook — same provider event processed once
// ===========================================================================

#[tokio::test]
async fn duplicate_webhook_event_is_idempotent() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };

    let event_id = format!("evt-{}", Uuid::new_v4());

    // First delivery: acquired.
    let first = sqlx::query(
        "INSERT INTO billing_webhook_idempotency (provider_event_id) VALUES ($1) ON CONFLICT DO NOTHING",
    )
    .bind(&event_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(first.rows_affected(), 1, "first delivery must be recorded");

    // Duplicate delivery: silently ignored.
    let second = sqlx::query(
        "INSERT INTO billing_webhook_idempotency (provider_event_id) VALUES ($1) ON CONFLICT DO NOTHING",
    )
    .bind(&event_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        second.rows_affected(),
        0,
        "duplicate delivery must be a no-op"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM billing_webhook_idempotency WHERE provider_event_id=$1",
    )
    .bind(&event_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

// ===========================================================================
// 4) duplicate usage request — same (tenant, request_id) inserts once
// ===========================================================================

#[tokio::test]
async fn duplicate_usage_request_inserted_once() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, _wallet) = setup_tenant_wallet(&pool, 0).await;

    let request_id = format!("req-{}", Uuid::new_v4());
    let event_id_1 = Uuid::new_v4();
    let event_id_2 = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO api_usage_events
         (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,pricing_version,request_id,status)
         VALUES ($1,$2,'http_check.one_time',1,1,1000,1000,'USD',1,$3,'CAPTURED')",
    )
    .bind(event_id_1)
    .bind(tenant_id)
    .bind(&request_id)
    .execute(&pool)
    .await
    .unwrap();

    let second = sqlx::query(
        "INSERT INTO api_usage_events
         (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,pricing_version,request_id,status)
         VALUES ($1,$2,'http_check.one_time',1,1,1000,1000,'USD',1,$3,'CAPTURED')",
    )
    .bind(event_id_2)
    .bind(tenant_id)
    .bind(&request_id)
    .execute(&pool)
    .await;
    assert!(
        second.is_err(),
        "duplicate (tenant_id, request_id) must violate UNIQUE and fail"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM api_usage_events WHERE tenant_id=$1 AND request_id=$2",
    )
    .bind(tenant_id)
    .bind(&request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "exactly one usage event per (tenant, request_id)");
}

// ===========================================================================
// 5) concurrent reservation — cannot overspend the wallet
// ===========================================================================

#[tokio::test]
async fn concurrent_wallet_reservations_cannot_overspend() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    // Wallet holds exactly enough for ONE reservation of 1_000.
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 1_000).await;

    let mut tasks = JoinSet::new();
    for i in 0..4 {
        let w = wallet.clone();
        let request_id = format!("part4-overspend-{}-{}", i, Uuid::new_v4());
        tasks.spawn(async move {
            w.reserve(
                tenant_id,
                1_000,
                "http_check.one_time",
                1_000,
                1,
                "USD",
                &request_id,
                Utc::now() + Duration::minutes(15),
            )
            .await
        });
    }

    let mut successes = 0;
    let mut failures = 0;
    while let Some(res) = tasks.join_next().await {
        match res.unwrap() {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
    }
    assert_eq!(successes, 1, "exactly one reservation must succeed");
    assert_eq!(failures, 3, "the rest must fail with insufficient balance");

    // Sanity: reserved amount does not exceed wallet balance.
    let reserved: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_units),0)::BIGINT
         FROM credit_reservations
         WHERE wallet_id=(SELECT id FROM user_wallets WHERE tenant_id=$1)
           AND status='PENDING' AND expires_at>now()",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        reserved <= 1_000,
        "reserved_units ({reserved}) must never exceed wallet balance"
    );
}
