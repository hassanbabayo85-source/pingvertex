//! Financial concurrency tests.
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
         VALUES ($1, $2, 'not-a-real-hash', 'Concurrency Test', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@concurrency.test", user_id.0))
    .execute(pool)
    .await
    .expect("insert test user");

    let tenant_repo = PostgresTenantRepository::new(pool.clone());
    let tenant = domain::Tenant::new(TenantId::new(), "Concurrency Test Tenant", user_id).unwrap();
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
            &format!("cap-{}", Uuid::new_v4()),
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

    let key = format!("topup-{}", Uuid::new_v4());

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
        let request_id = format!("overspend-{}-{}", i, Uuid::new_v4());
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

// ===========================================================================
// Adversarial concurrency + financial invariant tests
// ===========================================================================

/// 100 concurrent beacons with the SAME request_id: exactly one
/// reservation is created, exactly one wallet transaction is written,
/// and the wallet is debited exactly once.
#[tokio::test]
async fn hundred_concurrent_same_request_id_debits_exactly_once() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 1_000).await;

    let shared_request_id = format!("hundred-same-{}", Uuid::new_v4());
    const N: usize = 100;
    const AMOUNT: i64 = 10;

    let mut tasks = JoinSet::new();
    for _ in 0..N {
        let w = wallet.clone();
        let req = shared_request_id.clone();
        tasks.spawn(async move {
            let outcome = w
                .reserve(
                    tenant_id,
                    AMOUNT,
                    "http_check.monitoring",
                    1,
                    1,
                    "USD",
                    &req,
                    Utc::now() + Duration::seconds(60),
                )
                .await?;
            let tx = w.capture(outcome.reservation.id, None, &[], None).await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>((
                outcome.reservation.id,
                tx.id,
                outcome.created,
            ))
        });
    }

    let mut reservation_ids = std::collections::HashSet::new();
    let mut tx_ids = std::collections::HashSet::new();
    let mut created_count = 0usize;
    let mut errors = 0usize;
    while let Some(res) = tasks.join_next().await {
        match res.expect("task must not panic") {
            Ok((rid, tid, created)) => {
                reservation_ids.insert(rid);
                tx_ids.insert(tid);
                if created {
                    created_count += 1;
                }
            }
            Err(_) => errors += 1,
        }
    }

    assert_eq!(
        errors, 0,
        "no errors expected — replays return the same row"
    );
    assert_eq!(
        reservation_ids.len(),
        1,
        "all 100 callers must observe exactly one reservation row"
    );
    assert_eq!(
        tx_ids.len(),
        1,
        "all 100 callers must observe exactly one wallet transaction"
    );
    assert_eq!(
        created_count, 1,
        "exactly one caller may report created=true"
    );

    let balance = wallet.find(tenant_id).await.unwrap().unwrap().balance_units;
    assert_eq!(balance, 1_000 - AMOUNT, "wallet debited exactly once");

    let debit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wallet_transactions
         WHERE tenant_id=$1 AND transaction_type='DEBIT'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(debit_count, 1, "exactly one DEBIT row for the whole burst");
}

/// 100 concurrent requests with DISTINCT request_ids against the same
/// wallet: each is an independent debit, and the wallet balance must
/// reflect the exact arithmetic sum.
#[tokio::test]
async fn hundred_concurrent_distinct_request_ids_against_same_wallet() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 1_000).await;

    const N: usize = 100;
    const AMOUNT: i64 = 5;

    let mut tasks = JoinSet::new();
    for i in 0..N {
        let w = wallet.clone();
        let req = format!("hundred-distinct-{}-{}", i, Uuid::new_v4());
        tasks.spawn(async move {
            let outcome = w
                .reserve(
                    tenant_id,
                    AMOUNT,
                    "http_check.monitoring",
                    1,
                    1,
                    "USD",
                    &req,
                    Utc::now() + Duration::seconds(60),
                )
                .await?;
            w.capture(outcome.reservation.id, None, &[], None).await
        });
    }

    let mut successes = 0usize;
    while let Some(res) = tasks.join_next().await {
        res.expect("task must not panic")
            .expect("call must not fail");
        successes += 1;
    }
    assert_eq!(successes, N);

    let balance = wallet.find(tenant_id).await.unwrap().unwrap().balance_units;
    assert_eq!(
        balance,
        1_000 - (N as i64) * AMOUNT,
        "each reservation must debit independently"
    );

    let debit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wallet_transactions
         WHERE tenant_id=$1 AND transaction_type='DEBIT'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(debit_count, N as i64, "exactly N DEBIT rows");
}

/// Simultaneous refund + top-up: both are credits against the same
/// wallet. Each must apply exactly once, and the arithmetic must be
/// exact — no money created, no money destroyed.
#[tokio::test]
async fn simultaneous_refund_and_top_up_both_apply_once() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 0).await;

    // Seed the wallet via a completed top-up that will be refundable.
    let seed_top_up = 5_000_i64;
    let checkout_id = format!("chk-{}", Uuid::new_v4());
    let session = domain::WalletTopUpSession {
        id: Uuid::new_v4(),
        tenant_id,
        provider: "paystack".to_string(),
        provider_checkout_id: checkout_id.clone(),
        checkout_url: "https://example.test/pay".to_string(),
        idempotency_key: format!("idem-{}", Uuid::new_v4()),
        provider_payment_id: None,
        amount_units: seed_top_up,
        refunded_amount_units: 0,
        currency: "USD".to_string(),
        status: "PENDING".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    wallet.create_top_up_session(&session).await.unwrap();
    wallet
        .complete_top_up_session(tenant_id, &checkout_id, "provider-pay-ref")
        .await
        .unwrap();
    wallet
        .credit(
            tenant_id,
            seed_top_up,
            WalletTransactionType::TopUp,
            &format!("topup:provider-pay-ref:{}", Uuid::new_v4()),
            Some("seed top-up"),
        )
        .await
        .unwrap();

    // Wallet now: 5_000.
    let before = wallet.find(tenant_id).await.unwrap().unwrap().balance_units;
    assert_eq!(before, seed_top_up);

    // Fire refund (credit) and top-up (credit) simultaneously.
    let refund_amount = 2_000_i64;
    let new_top_up_amount = 3_000_i64;
    let refund_id = format!("refund-{}", Uuid::new_v4());
    let new_top_up_key = format!("newtopup-{}", Uuid::new_v4());

    let wallet_r = wallet.clone();
    let refund_task = tokio::spawn(async move {
        wallet_r
            .record_verified_refund(tenant_id, "provider-pay-ref", refund_amount, &refund_id)
            .await
    });
    let wallet_t = wallet.clone();
    let topup_task = tokio::spawn(async move {
        wallet_t
            .credit(
                tenant_id,
                new_top_up_amount,
                WalletTransactionType::TopUp,
                &new_top_up_key,
                Some("concurrent top-up"),
            )
            .await
    });

    let (refund_res, topup_res) = tokio::join!(refund_task, topup_task);
    assert!(
        refund_res.unwrap().unwrap(),
        "refund must be recorded successfully"
    );
    topup_res.unwrap().expect("top-up must succeed");

    let after = wallet.find(tenant_id).await.unwrap().unwrap().balance_units;
    assert_eq!(
        after,
        seed_top_up + refund_amount + new_top_up_amount,
        "both concurrent credits must apply exactly once"
    );

    // The invariant: balance == SUM(wallet_transactions.amount_units)
    let ledger_sum: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_units),0)::BIGINT
         FROM wallet_transactions WHERE tenant_id=$1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ledger_sum, after,
        "FINANCIAL INVARIANT VIOLATED: balance must equal SUM(wallet_transactions)"
    );

    // Every ledger row must record a non-negative balance_after_units.
    let negative: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wallet_transactions
         WHERE tenant_id=$1 AND balance_after_units < 0",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(negative, 0);
}

/// Financial invariant under a mixed workload:
///   initial + credits - captures - refunds - adjustments == current
///
/// This is the explicit test for the SECURITY INVARIANT. The
/// reconciliation worker checks the same identity at runtime; this test
/// proves it holds across every operation kind on a fresh wallet.
#[tokio::test]
async fn financial_invariant_holds_after_mixed_workload() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 0).await;

    // Three top-ups.
    for i in 0..3 {
        wallet
            .credit(
                tenant_id,
                10_000,
                WalletTransactionType::TopUp,
                &format!("inv-topup-{}-{}", i, Uuid::new_v4()),
                Some("test top-up"),
            )
            .await
            .unwrap();
    }

    // One positive admin adjustment.
    wallet
        .adjust_balance(
            tenant_id,
            5_000,
            &format!("inv-adj-{}", Uuid::new_v4()),
            Some("test adjustment"),
        )
        .await
        .unwrap();

    // Two reservations + captures (debits).
    for i in 0..2 {
        let outcome = wallet
            .reserve(
                tenant_id,
                1_000,
                "http_check.monitoring",
                500,
                1,
                "USD",
                &format!("inv-debit-{}-{}", i, Uuid::new_v4()),
                Utc::now() + Duration::seconds(60),
            )
            .await
            .unwrap();
        wallet
            .capture(outcome.reservation.id, None, &[], None)
            .await
            .unwrap();
    }

    // One promotional credit.
    wallet
        .grant_promotional_credit(
            tenant_id,
            2_000,
            Utc::now() + Duration::days(30),
            &format!("inv-promo-{}", Uuid::new_v4()),
            Some("test promotional credit"),
        )
        .await
        .unwrap();

    // Expected: 3 * 10_000 + 5_000 - 2 * 1_000 + 2_000 = 35_000
    let expected: i64 = 3 * 10_000 + 5_000 - 2 * 1_000 + 2_000;
    let balance = wallet.find(tenant_id).await.unwrap().unwrap().balance_units;
    assert_eq!(balance, expected);

    // The invariant.
    let ledger_sum: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_units),0)::BIGINT
         FROM wallet_transactions WHERE tenant_id=$1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ledger_sum, balance,
        "FINANCIAL INVARIANT VIOLATED: initial + credits - debits == current_balance"
    );

    // Cross-check against the reconciler's own inequality: no negative
    // balance_after_units, and every row's sign matches its type.
    let bad_sign: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wallet_transactions WHERE tenant_id=$1
         AND ((transaction_type IN ('TOP_UP','REFUND','PROMOTIONAL_CREDIT')
               AND amount_units <= 0)
           OR (transaction_type IN ('DEBIT','EXPIRATION')
               AND amount_units >= 0))",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(bad_sign, 0, "every ledger row must have the correct sign");

    // Sanity: the reservation rows themselves must be terminal.
    let non_terminal: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM credit_reservations
         WHERE wallet_id=(SELECT id FROM user_wallets WHERE tenant_id=$1)
           AND status NOT IN ('CAPTURED','RELEASED','EXPIRED')",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(non_terminal, 0, "no reservation may remain PENDING");
}
