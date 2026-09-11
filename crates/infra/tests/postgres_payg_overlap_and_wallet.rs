//! PostgreSQL integration tests for:
//!
//!   1. Migration 0104 (`prevent_ambiguous_payg_pricing_overlap` trigger),
//!      which enforces absolute PAYG pricing overlap at the database level.
//!   2. `PostgresWalletRepository::mark_top_up_session_failed` — the
//!      PENDING→FAILED transition added for the Paystack `charge.failed`
//!      webhook.
//!   3. `PostgresWalletRepository::find_top_up_session_global` — the
//!      (provider, provider_checkout_id) lookup used by the Paystack
//!      webhook to resolve a top-up session without a pre-known tenant id.
//!
//! These tests connect to the Docker Postgres at the URL in `.env.test`
//! (`127.0.0.1:5433`, database `uptime_monitor_test`). Migration 0104 must
//! already be applied to that database.
//!
//! Isolation strategy:
//!   * Trigger tests wrap every write in `BEGIN ... ROLLBACK`, so nothing
//!     leaks into the test database regardless of pass/fail.
//!   * Wallet session tests use unique `provider_checkout_id` values and
//!     delete their seeded row at the end. If a test panics between seed
//!     and cleanup, the row is harmless (unique id, test-only DB).
//!   * Every test uses a unique `operation` string for the pricing tests
//!     so parallel tests never see each other's rows.
//!
//! Graceful skip: if `DATABASE_URL` is not set or the test Postgres is
//! unreachable, each test prints a SKIP line and returns Ok. This keeps
//! `cargo test --workspace` green on machines without Docker running.

use application::ports::WalletRepository;
use infra::repositories::postgres_wallet_repository::PostgresWalletRepository;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_env_test() {
    for candidate in &["../../.env.test", ".env.test", "../.env.test"] {
        if std::path::Path::new(candidate).exists() {
            dotenvy::from_filename(candidate).ok();
            return;
        }
    }
}

async fn pool_or_skip() -> Option<PgPool> {
    load_env_test();
    let url = match std::env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => {
            eprintln!(
                "SKIP: DATABASE_URL not set (see .env.test); skipping PostgreSQL integration test."
            );
            return None;
        }
    };
    match PgPool::connect(&url).await {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!(
                "SKIP: cannot connect to test Postgres ({e}); skipping PostgreSQL integration test."
            );
            None
        }
    }
}

async fn any_tenant_id(pool: &PgPool) -> Option<Uuid> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM tenants LIMIT 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Unique operation string so trigger tests never see each other's rows.
fn unique_op() -> String {
    format!("test.op.{}", Uuid::new_v4().simple())
}

/// Large random version that cannot collide with real pricing versions
/// (which are 1..100) nor with another concurrently-running test.
fn unique_version() -> i64 {
    let bytes = Uuid::new_v4().into_bytes();
    let n = i64::from_le_bytes(bytes[..8].try_into().unwrap());
    // Map into [100_000_000_000, 1_000_000_000_000).
    (n.unsigned_abs() as i64 % 900_000_000_000) + 100_000_000_000
}

async fn seed_wallet_session(
    pool: &PgPool,
    tenant_id: Uuid,
    provider: &str,
    checkout_id: &str,
    status: &str,
    amount_units: i64,
    provider_payment_id: Option<&str>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO wallet_top_up_sessions
          (id, tenant_id, provider, provider_checkout_id, provider_payment_id,
           amount_units, refunded_amount_units, currency, status)
         VALUES ($1, $2, $3, $4, $5, $6, 0, 'USD', $7)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(provider)
    .bind(checkout_id)
    .bind(provider_payment_id)
    .bind(amount_units)
    .bind(status)
    .execute(pool)
    .await
    .expect("seed wallet_top_up_sessions");
    id
}

async fn cleanup_wallet_session(pool: &PgPool, id: Uuid) {
    sqlx::query("DELETE FROM wallet_top_up_sessions WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .ok();
}

// ===========================================================================
// Migration 0104: prevent_ambiguous_payg_pricing_overlap
// ===========================================================================

/// Same exact context (all NULL), different version → allowed (versioning).
#[tokio::test]
async fn trigger_allows_same_context_versioning() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let mut tx = pool.begin().await.unwrap();
    let op = unique_op();

    for i in 0..2 {
        sqlx::query(
            "INSERT INTO payg_pricing_configs
              (id, version, operation, unit_name, unit_price, currency, effective_from, active)
             VALUES ($1, $2, $3, 'check', 2, 'USD',
                     now() + make_interval(days => $4::int), true)",
        )
        .bind(Uuid::new_v4())
        .bind(unique_version())
        .bind(&op)
        .bind(i as i32)
        .execute(&mut *tx)
        .await
        .expect("same-context versioning must be allowed");
    }

    tx.rollback().await.unwrap();
}

/// Two specificity-1 rules whose wildcard contexts overlap → 2nd rejected.
#[tokio::test]
async fn trigger_rejects_equal_specificity_overlap() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let mut tx = pool.begin().await.unwrap();
    let op = unique_op();

    // Rule A: spec 1 via region only.
    sqlx::query(
        "INSERT INTO payg_pricing_configs
          (id, version, operation, unit_name, unit_price, currency, effective_from, active, region)
         VALUES ($1, $2, $3, 'check', 2, 'USD', now(), true, 'us-east')",
    )
    .bind(Uuid::new_v4())
    .bind(unique_version())
    .bind(&op)
    .execute(&mut *tx)
    .await
    .unwrap();

    // Rule B: spec 1 via monitor_kind only — wildcard-overlaps Rule A.
    let result = sqlx::query(
        "INSERT INTO payg_pricing_configs
          (id, version, operation, unit_name, unit_price, currency, effective_from, active, monitor_kind)
         VALUES ($1, $2, $3, 'check', 2, 'USD', now(), true, 'http')",
    )
    .bind(Uuid::new_v4())
    .bind(unique_version())
    .bind(&op)
    .execute(&mut *tx)
    .await;

    assert!(
        result.is_err(),
        "equal-specificity overlap must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("ambiguous PAYG pricing rule"),
        "expected ambiguous-rule error, got: {msg}"
    );

    tx.rollback().await.unwrap();
}

/// Spec-2 rule coexists with a spec-1 rule that wildcard-overlaps it.
#[tokio::test]
async fn trigger_allows_higher_specificity_coexist() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let mut tx = pool.begin().await.unwrap();
    let op = unique_op();

    // Rule A: spec 1 (region only).
    sqlx::query(
        "INSERT INTO payg_pricing_configs
          (id, version, operation, unit_name, unit_price, currency, effective_from, active, region)
         VALUES ($1, $2, $3, 'check', 2, 'USD', now(), true, 'us-east')",
    )
    .bind(Uuid::new_v4())
    .bind(unique_version())
    .bind(&op)
    .execute(&mut *tx)
    .await
    .unwrap();

    // Rule B: spec 2 (region + account_type). Wildcard-overlaps A on the
    // account_type dimension, but has strictly greater specificity → allowed.
    sqlx::query(
        "INSERT INTO payg_pricing_configs
          (id, version, operation, unit_name, unit_price, currency, effective_from, active,
           region, account_type)
         VALUES ($1, $2, $3, 'check', 2, 'USD', now(), true, 'us-east', 'company')",
    )
    .bind(Uuid::new_v4())
    .bind(unique_version())
    .bind(&op)
    .execute(&mut *tx)
    .await
    .expect("higher-specificity rule must be allowed to coexist");

    tx.rollback().await.unwrap();
}

/// Two FUTURE-effective rules at equal specificity and overlapping context
/// → 2nd rejected. This is the gap the application-level check does not
/// cover; only the DB trigger catches it.
#[tokio::test]
async fn trigger_rejects_future_effective_overlap() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let mut tx = pool.begin().await.unwrap();
    let op = unique_op();

    // Rule A: spec 1, effective 100 days from now.
    sqlx::query(
        "INSERT INTO payg_pricing_configs
          (id, version, operation, unit_name, unit_price, currency, effective_from, active, region)
         VALUES ($1, $2, $3, 'check', 2, 'USD', now() + interval '100 days', true, 'us-east')",
    )
    .bind(Uuid::new_v4())
    .bind(unique_version())
    .bind(&op)
    .execute(&mut *tx)
    .await
    .unwrap();

    // Rule B: spec 1, effective 101 days from now, wildcard-overlaps A.
    let result = sqlx::query(
        "INSERT INTO payg_pricing_configs
          (id, version, operation, unit_name, unit_price, currency, effective_from, active, monitor_kind)
         VALUES ($1, $2, $3, 'check', 2, 'USD', now() + interval '101 days', true, 'http')",
    )
    .bind(Uuid::new_v4())
    .bind(unique_version())
    .bind(&op)
    .execute(&mut *tx)
    .await;

    assert!(
        result.is_err(),
        "future-effective equal-specificity overlap must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("ambiguous PAYG pricing rule"), "got: {msg}");

    tx.rollback().await.unwrap();
}

/// Two specificity-1 rules whose quantity ranges overlap → 2nd rejected.
#[tokio::test]
async fn trigger_rejects_quantity_range_overlap() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let mut tx = pool.begin().await.unwrap();
    let op = unique_op();

    // Rule A: quantity [0, 100].
    sqlx::query(
        "INSERT INTO payg_pricing_configs
          (id, version, operation, unit_name, unit_price, currency, effective_from, active,
           min_quantity, max_quantity)
         VALUES ($1, $2, $3, 'check', 2, 'USD', now(), true, 0, 100)",
    )
    .bind(Uuid::new_v4())
    .bind(unique_version())
    .bind(&op)
    .execute(&mut *tx)
    .await
    .unwrap();

    // Rule B: quantity [50, 200] — overlaps [50, 100].
    let result = sqlx::query(
        "INSERT INTO payg_pricing_configs
          (id, version, operation, unit_name, unit_price, currency, effective_from, active,
           min_quantity, max_quantity)
         VALUES ($1, $2, $3, 'check', 2, 'USD', now(), true, 50, 200)",
    )
    .bind(Uuid::new_v4())
    .bind(unique_version())
    .bind(&op)
    .execute(&mut *tx)
    .await;

    assert!(
        result.is_err(),
        "overlapping quantity ranges at equal specificity must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("ambiguous PAYG pricing rule"), "got: {msg}");

    tx.rollback().await.unwrap();
}

// ===========================================================================
// PostgresWalletRepository::mark_top_up_session_failed
// ===========================================================================

#[tokio::test]
async fn mark_top_up_session_failed_transitions_pending_to_failed() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let Some(tenant_id) = any_tenant_id(&pool).await else {
        eprintln!("SKIP: no tenants in test DB; skipping wallet session test.");
        return;
    };
    let repo = PostgresWalletRepository::new(pool.clone());
    let checkout = format!("test-pv-{}", Uuid::new_v4().simple());

    let id = seed_wallet_session(
        &pool, tenant_id, "paystack", &checkout, "PENDING", 5_000_000, None,
    )
    .await;

    let marked = repo
        .mark_top_up_session_failed(tenant_id, &checkout, "integration test reason")
        .await
        .expect("mark_top_up_session_failed must succeed");
    assert!(marked, "PENDING session must transition to FAILED");

    let status: String =
        sqlx::query_scalar("SELECT status FROM wallet_top_up_sessions WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "FAILED");

    cleanup_wallet_session(&pool, id).await;
}

#[tokio::test]
async fn mark_top_up_session_failed_leaves_completed_untouched() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let Some(tenant_id) = any_tenant_id(&pool).await else {
        eprintln!("SKIP: no tenants in test DB; skipping wallet session test.");
        return;
    };
    let repo = PostgresWalletRepository::new(pool.clone());
    let checkout = format!("test-pv-{}", Uuid::new_v4().simple());

    let id = seed_wallet_session(
        &pool,
        tenant_id,
        "paystack",
        &checkout,
        "COMPLETED",
        5_000_000,
        Some("12345"),
    )
    .await;

    let marked = repo
        .mark_top_up_session_failed(tenant_id, &checkout, "late failure")
        .await
        .expect("mark_top_up_session_failed must not error on COMPLETED");
    assert!(!marked, "COMPLETED session must not be marked FAILED");

    let status: String =
        sqlx::query_scalar("SELECT status FROM wallet_top_up_sessions WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "COMPLETED", "COMPLETED must be left untouched");

    cleanup_wallet_session(&pool, id).await;
}

#[tokio::test]
async fn mark_top_up_session_failed_is_idempotent() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let Some(tenant_id) = any_tenant_id(&pool).await else {
        eprintln!("SKIP: no tenants in test DB; skipping wallet session test.");
        return;
    };
    let repo = PostgresWalletRepository::new(pool.clone());
    let checkout = format!("test-pv-{}", Uuid::new_v4().simple());

    let id = seed_wallet_session(
        &pool, tenant_id, "paystack", &checkout, "PENDING", 5_000_000, None,
    )
    .await;

    let first = repo
        .mark_top_up_session_failed(tenant_id, &checkout, "first call")
        .await
        .unwrap();
    assert!(first, "first call must transition PENDING→FAILED");

    let second = repo
        .mark_top_up_session_failed(tenant_id, &checkout, "second call")
        .await
        .unwrap();
    assert!(!second, "second call must report no change (idempotent)");

    cleanup_wallet_session(&pool, id).await;
}

// ===========================================================================
// PostgresWalletRepository::find_top_up_session_global
// ===========================================================================

#[tokio::test]
async fn find_top_up_session_global_finds_by_provider_and_checkout() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let Some(tenant_id) = any_tenant_id(&pool).await else {
        eprintln!("SKIP: no tenants in test DB; skipping wallet session test.");
        return;
    };
    let repo = PostgresWalletRepository::new(pool.clone());
    let checkout = format!("test-pv-{}", Uuid::new_v4().simple());

    let id = seed_wallet_session(
        &pool, tenant_id, "paystack", &checkout, "PENDING", 5_000_000, None,
    )
    .await;

    let found = repo
        .find_top_up_session_global("paystack", &checkout)
        .await
        .expect("find_top_up_session_global must succeed")
        .expect("must find the seeded session");
    assert_eq!(found.id, id);
    assert_eq!(found.tenant_id, tenant_id);
    assert_eq!(found.provider, "paystack");
    assert_eq!(found.provider_checkout_id, checkout);
    assert_eq!(found.amount_units, 5_000_000);
    assert_eq!(found.status, "PENDING");

    cleanup_wallet_session(&pool, id).await;
}

#[tokio::test]
async fn find_top_up_session_global_returns_none_for_unknown() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let repo = PostgresWalletRepository::new(pool.clone());

    let unknown = format!("test-pv-unknown-{}", Uuid::new_v4().simple());
    let found = repo
        .find_top_up_session_global("paystack", &unknown)
        .await
        .expect("lookup must not error on unknown reference");
    assert!(found.is_none(), "unknown reference must return None");
}

#[tokio::test]
async fn find_top_up_session_global_is_provider_scoped() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let Some(tenant_id) = any_tenant_id(&pool).await else {
        eprintln!("SKIP: no tenants in test DB; skipping wallet session test.");
        return;
    };
    let repo = PostgresWalletRepository::new(pool.clone());
    let checkout = format!("test-pv-{}", Uuid::new_v4().simple());

    let id = seed_wallet_session(
        &pool, tenant_id, "paystack", &checkout, "PENDING", 5_000_000, None,
    )
    .await;

    // Same checkout id, different provider → must return None.
    let found = repo
        .find_top_up_session_global("some_other_provider", &checkout)
        .await
        .unwrap();
    assert!(
        found.is_none(),
        "lookup must be scoped to the provider, not just the checkout id"
    );

    cleanup_wallet_session(&pool, id).await;
}
