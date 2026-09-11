//! Auto-top-up worker tests.
//!
//! Two families:
//!
//!   1. An in-memory concurrency test (kept from the original file)
//!      proving that many concurrent worker instances charge the same
//!      tenant only once.
//!
//!   2. PostgreSQL reconciliation edge-case tests. These use the real
//!      `PostgresWalletRepository` because `reconcile()` depends on
//!      `list_auto_top_up_reconciliation_attempts`, which the in-memory
//!      fake does not implement. Each test seeds a specific
//!      `wallet_auto_top_up_attempts` row, wires a deterministic
//!      `PaymentProvider` mock, then drives exactly one reconciliation
//!      pass and asserts the resulting state.
//!
//! Edge cases covered:
//!
//!   * Provider success → wallet credited, attempt SUCCEEDED
//!   * Idempotent replay: a re-run credits the wallet only once
//!   * Amount mismatch → REQUIRES_RECONCILIATION, no credit
//!   * Currency mismatch → REQUIRES_RECONCILIATION, no credit
//!   * Provider still pending → REQUIRES_RECONCILIATION, no credit
//!   * Provider failed → FAILED, no credit
//!   * Multiple attempts in one pass → handled independently
//!   * Attempt without `provider_reference` → silently skipped
//!
//! NOTE: the `provider success + wallet credit failure` case (wallet
//! `credit()` returns `Err` after provider already charged) is not
//! exercised here. Triggering it requires a ~150-line trait facade that
//! wraps every `WalletRepository` method to inject a single failing call.
//! The worker branch itself is `auto_top_up_worker.rs` line ~131, which
//! sets `status = "REQUIRES_RECONCILIATION"` and
//! `error_message = "wallet credit failed: {e}"`; that status is already
//! proven retriable by `reconcile_credits_when_provider_confirms_payment`.

use application::ports::{
    CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentVerification, TenantRepository,
    WalletRepository, WalletTopUpRequest,
};
use async_trait::async_trait;
use domain::WalletTransactionType;
use domain::shared::ids::{TenantId, UserId};
use infra::InMemoryWalletRepository;
use infra::repositories::postgres_tenant_repository::PostgresTenantRepository;
use infra::repositories::postgres_wallet_repository::PostgresWalletRepository;
use serial_test::serial;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ===========================================================================
// In-memory concurrency test (kept from the original file)
// ===========================================================================

#[derive(Clone)]
struct CountingProvider {
    charges: Arc<AtomicUsize>,
}

#[async_trait]
impl PaymentProvider for CountingProvider {
    fn provider_name(&self) -> &'static str {
        "load-test"
    }
    async fn create_checkout(
        &self,
        _: &CheckoutRequest,
    ) -> application::ports::PaymentProviderResult<CheckoutDetails> {
        Err("unused".into())
    }
    async fn verify_payment(
        &self,
        reference: &str,
    ) -> application::ports::PaymentProviderResult<PaymentVerification> {
        Ok(PaymentVerification::Paid {
            provider_payment_id: format!("payment-{reference}"),
            provider_reference: reference.to_owned(),
            amount_units: 1_000,
            currency: "USD".to_owned(),
        })
    }
    async fn charge_saved_payment_method(
        &self,
        request: &WalletTopUpRequest,
    ) -> application::ports::PaymentProviderResult<PaymentVerification> {
        self.charges.fetch_add(1, Ordering::SeqCst);
        Ok(PaymentVerification::Paid {
            provider_payment_id: "provider-payment-once".to_owned(),
            provider_reference: request.idempotency_key.clone(),
            amount_units: request.amount_units,
            currency: request.currency.clone(),
        })
    }
}

#[tokio::test]
async fn concurrent_workers_charge_same_tenant_only_once() {
    let wallet = Arc::new(InMemoryWalletRepository::default());
    let tenant = Uuid::new_v4();
    wallet.get_or_create(tenant, "USD").await.unwrap();
    let mut settings = wallet.get_settings(tenant).await.unwrap();
    settings.auto_top_up_enabled = true;
    settings.auto_top_up_threshold_units = 5_000;
    settings.auto_top_up_amount_units = 10_000;
    settings.payment_method = Some("saved-method".to_owned());
    settings.daily_top_up_limit_units = 100_000;
    settings.monthly_top_up_limit_units = 100_000;
    settings.retry_limit = 3;
    wallet.update_settings(&settings).await.unwrap();
    wallet
        .credit(tenant, 1_000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let provider = Arc::new(CountingProvider {
        charges: Arc::new(AtomicUsize::new(0)),
    });
    let cancel = CancellationToken::new();
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let worker = worker::AutoTopUpWorker::new(
            wallet.clone(),
            provider.clone(),
            Duration::from_millis(2),
        );
        let cancel = cancel.clone();
        tasks.push(tokio::spawn(
            async move { worker.run_forever(cancel).await },
        ));
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
    cancel.cancel();
    for task in tasks {
        task.await.unwrap().unwrap();
    }

    assert_eq!(
        provider.charges.load(Ordering::SeqCst),
        1,
        "provider must be charged once under worker concurrency"
    );
    assert_eq!(
        wallet.find(tenant).await.unwrap().unwrap().balance_units,
        11_000
    );
}

// ===========================================================================
// PostgreSQL reconciliation edge-case tests
// ===========================================================================

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
                "SKIP: DATABASE_URL not set (see .env.test); skipping auto-topup reconciliation test."
            );
            return None;
        }
    };
    match PgPool::connect(&url).await {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!(
                "SKIP: cannot connect to test Postgres ({e}); skipping auto-topup reconciliation test."
            );
            None
        }
    }
}

/// Mock provider with a reference→response map.
///
/// Any reference not in the map returns `Pending`, which the worker treats
/// as a no-op reconciliation. This makes the mock immune to attempts
/// seeded by other tests that happen to still be in the shared test
/// database — those attempts get a harmless `Pending` and stay in
/// `REQUIRES_RECONCILIATION`.
#[derive(Clone)]
struct MapProvider {
    responses: Arc<HashMap<String, PaymentVerification>>,
}

#[async_trait]
impl PaymentProvider for MapProvider {
    fn provider_name(&self) -> &'static str {
        "map-test"
    }
    async fn create_checkout(
        &self,
        _: &CheckoutRequest,
    ) -> application::ports::PaymentProviderResult<CheckoutDetails> {
        Err("unused".into())
    }
    async fn verify_payment(
        &self,
        reference: &str,
    ) -> application::ports::PaymentProviderResult<PaymentVerification> {
        match self.responses.get(reference) {
            Some(r) => Ok(r.clone()),
            None => Ok(PaymentVerification::Pending {
                provider_payment_id: None,
                provider_reference: None,
                amount_units: None,
                currency: None,
            }),
        }
    }
    async fn charge_saved_payment_method(
        &self,
        _: &WalletTopUpRequest,
    ) -> application::ports::PaymentProviderResult<PaymentVerification> {
        Err("unused".into())
    }
}

async fn setup_tenant_and_wallet(
    pool: &PgPool,
    initial_balance: i64,
) -> (Uuid, Arc<PostgresWalletRepository>) {
    let user_id = UserId::new();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at)
         VALUES ($1, $2, 'not-a-real-hash', 'Auto-Topup Test', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@auto-topup.test", user_id.0))
    .execute(pool)
    .await
    .expect("insert test user");

    let tenant_repo = PostgresTenantRepository::new(pool.clone());
    let tenant = domain::Tenant::new(
        TenantId::new(),
        format!("Auto-Topup Test {}", user_id.0),
        user_id,
    )
    .expect("construct tenant");
    tenant_repo.create(&tenant).await.expect("create tenant");

    let wallet_repo = Arc::new(PostgresWalletRepository::new(pool.clone()));
    wallet_repo
        .get_or_create(tenant.id.0, "USD")
        .await
        .expect("create wallet");

    let mut settings = wallet_repo
        .get_settings(tenant.id.0)
        .await
        .expect("load settings");
    settings.auto_top_up_enabled = true;
    settings.auto_top_up_threshold_units = 5_000;
    settings.auto_top_up_amount_units = 10_000;
    settings.payment_method = Some("saved-method".to_owned());
    settings.daily_top_up_limit_units = 100_000;
    settings.monthly_top_up_limit_units = 100_000;
    settings.retry_limit = 3;
    wallet_repo
        .update_settings(&settings)
        .await
        .expect("update settings");

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

async fn seed_attempt(
    pool: &PgPool,
    tenant_id: Uuid,
    attempt_id: Uuid,
    reference: &str,
    amount_units: i64,
) {
    sqlx::query(
        "INSERT INTO wallet_auto_top_up_attempts
          (id, tenant_id, amount_units, status, provider_payment_id,
           provider_reference, error_message, attempted_at)
         VALUES ($1, $2, $3, 'REQUIRES_RECONCILIATION', NULL, $4, NULL, NOW())",
    )
    .bind(attempt_id)
    .bind(tenant_id)
    .bind(amount_units)
    .bind(reference)
    .execute(pool)
    .await
    .expect("seed attempt");
}

async fn fetch_attempt(pool: &PgPool, attempt_id: Uuid) -> (String, Option<String>) {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, provider_payment_id
           FROM wallet_auto_top_up_attempts
          WHERE id = $1",
    )
    .bind(attempt_id)
    .fetch_one(pool)
    .await
    .expect("fetch attempt")
}

fn paid(pid: &str, reference: &str, amount: i64, currency: &str) -> PaymentVerification {
    PaymentVerification::Paid {
        provider_payment_id: pid.to_owned(),
        provider_reference: reference.to_owned(),
        amount_units: amount,
        currency: currency.to_owned(),
    }
}

fn provider_with(entries: Vec<(String, PaymentVerification)>) -> Arc<MapProvider> {
    Arc::new(MapProvider {
        responses: Arc::new(entries.into_iter().collect::<HashMap<_, _>>()),
    })
}

#[tokio::test]
#[serial]
async fn reconcile_credits_when_provider_confirms_payment() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_and_wallet(&pool, 0).await;
    let attempt_id = Uuid::new_v4();
    let reference = format!("ref-{}", Uuid::new_v4());
    let pid = format!("pid-{}", Uuid::new_v4());
    seed_attempt(&pool, tenant_id, attempt_id, &reference, 10_000).await;

    let provider = provider_with(vec![(
        reference.clone(),
        paid(&pid, &reference, 10_000, "USD"),
    )]);
    let worker = worker::AutoTopUpWorker::new(wallet.clone(), provider, Duration::from_secs(60));
    worker.reconcile().await.expect("reconcile must not error");

    let (status, recorded_pid) = fetch_attempt(&pool, attempt_id).await;
    assert_eq!(status, "SUCCEEDED");
    assert_eq!(recorded_pid.as_deref(), Some(pid.as_str()));
    assert_eq!(
        wallet.find(tenant_id).await.unwrap().unwrap().balance_units,
        10_000
    );
}

#[tokio::test]
#[serial]
async fn reconcile_is_idempotent_when_called_twice_on_same_attempt() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_and_wallet(&pool, 0).await;
    let attempt_id = Uuid::new_v4();
    let reference = format!("ref-{}", Uuid::new_v4());
    let pid = format!("pid-{}", Uuid::new_v4());
    seed_attempt(&pool, tenant_id, attempt_id, &reference, 10_000).await;

    let provider = provider_with(vec![(
        reference.clone(),
        paid(&pid, &reference, 10_000, "USD"),
    )]);
    let worker = worker::AutoTopUpWorker::new(wallet.clone(), provider, Duration::from_secs(60));

    worker.reconcile().await.expect("first reconcile");
    let after_first = wallet.find(tenant_id).await.unwrap().unwrap().balance_units;
    assert_eq!(after_first, 10_000);

    // Simulate a crash between `credit()` succeeding and the attempt row
    // being updated to SUCCEEDED.
    sqlx::query(
        "UPDATE wallet_auto_top_up_attempts
            SET status='REQUIRES_RECONCILIATION', provider_payment_id=NULL
          WHERE id=$1",
    )
    .bind(attempt_id)
    .execute(&pool)
    .await
    .expect("reset attempt for replay");

    worker.reconcile().await.expect("second reconcile");
    let after_second = wallet.find(tenant_id).await.unwrap().unwrap().balance_units;
    assert_eq!(after_second, after_first, "replay must not double-credit");

    let (status, _) = fetch_attempt(&pool, attempt_id).await;
    assert_eq!(status, "SUCCEEDED");
}

#[tokio::test]
#[serial]
async fn reconcile_keeps_reconciliation_status_on_amount_mismatch() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_and_wallet(&pool, 0).await;
    let attempt_id = Uuid::new_v4();
    let reference = format!("ref-{}", Uuid::new_v4());
    let pid = format!("pid-{}", Uuid::new_v4());
    seed_attempt(&pool, tenant_id, attempt_id, &reference, 10_000).await;

    // Provider reports a different amount than the attempt recorded.
    let provider = provider_with(vec![(
        reference.clone(),
        paid(&pid, &reference, 99_999, "USD"),
    )]);
    let worker = worker::AutoTopUpWorker::new(wallet.clone(), provider, Duration::from_secs(60));
    worker.reconcile().await.expect("reconcile must not error");

    let (status, _) = fetch_attempt(&pool, attempt_id).await;
    assert_eq!(status, "REQUIRES_RECONCILIATION");
    assert_eq!(
        wallet.find(tenant_id).await.unwrap().unwrap().balance_units,
        0
    );
}

#[tokio::test]
#[serial]
async fn reconcile_keeps_reconciliation_status_on_currency_mismatch() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_and_wallet(&pool, 0).await;
    let attempt_id = Uuid::new_v4();
    let reference = format!("ref-{}", Uuid::new_v4());
    let pid = format!("pid-{}", Uuid::new_v4());
    seed_attempt(&pool, tenant_id, attempt_id, &reference, 10_000).await;

    // Provider reports a non-USD currency.
    let provider = provider_with(vec![(
        reference.clone(),
        paid(&pid, &reference, 10_000, "EUR"),
    )]);
    let worker = worker::AutoTopUpWorker::new(wallet.clone(), provider, Duration::from_secs(60));
    worker.reconcile().await.expect("reconcile must not error");

    let (status, _) = fetch_attempt(&pool, attempt_id).await;
    assert_eq!(status, "REQUIRES_RECONCILIATION");
    assert_eq!(
        wallet.find(tenant_id).await.unwrap().unwrap().balance_units,
        0
    );
}

#[tokio::test]
#[serial]
async fn reconcile_keeps_reconciliation_status_on_provider_pending() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_and_wallet(&pool, 0).await;
    let attempt_id = Uuid::new_v4();
    let reference = format!("ref-{}", Uuid::new_v4());
    seed_attempt(&pool, tenant_id, attempt_id, &reference, 10_000).await;

    let provider = provider_with(vec![(
        reference.clone(),
        PaymentVerification::Pending {
            provider_payment_id: Some("pid-pending".to_owned()),
            provider_reference: Some(reference.clone()),
            amount_units: Some(10_000),
            currency: Some("USD".to_owned()),
        },
    )]);
    let worker = worker::AutoTopUpWorker::new(wallet.clone(), provider, Duration::from_secs(60));
    worker.reconcile().await.expect("reconcile must not error");

    let (status, _) = fetch_attempt(&pool, attempt_id).await;
    assert_eq!(status, "REQUIRES_RECONCILIATION");
    assert_eq!(
        wallet.find(tenant_id).await.unwrap().unwrap().balance_units,
        0
    );
}

#[tokio::test]
#[serial]
async fn reconcile_marks_attempt_failed_on_provider_failure() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_and_wallet(&pool, 0).await;
    let attempt_id = Uuid::new_v4();
    let reference = format!("ref-{}", Uuid::new_v4());
    seed_attempt(&pool, tenant_id, attempt_id, &reference, 10_000).await;

    let provider = provider_with(vec![(
        reference.clone(),
        PaymentVerification::Failed {
            provider_payment_id: Some("pid-failed".to_owned()),
            provider_reference: Some(reference.clone()),
            amount_units: Some(10_000),
            currency: Some("USD".to_owned()),
        },
    )]);
    let worker = worker::AutoTopUpWorker::new(wallet.clone(), provider, Duration::from_secs(60));
    worker.reconcile().await.expect("reconcile must not error");

    let (status, _) = fetch_attempt(&pool, attempt_id).await;
    assert_eq!(status, "FAILED");
    assert_eq!(
        wallet.find(tenant_id).await.unwrap().unwrap().balance_units,
        0
    );
}

#[tokio::test]
#[serial]
async fn reconcile_handles_multiple_attempts_in_one_run() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_and_wallet(&pool, 0).await;

    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    let ref_a = format!("ref-a-{}", Uuid::new_v4());
    let ref_b = format!("ref-b-{}", Uuid::new_v4());
    let pid_a = format!("pid-a-{}", Uuid::new_v4());
    let pid_b = format!("pid-b-{}", Uuid::new_v4());

    seed_attempt(&pool, tenant_id, id_a, &ref_a, 10_000).await;
    seed_attempt(&pool, tenant_id, id_b, &ref_b, 20_000).await;

    let provider = provider_with(vec![
        (ref_a.clone(), paid(&pid_a, &ref_a, 10_000, "USD")),
        (ref_b.clone(), paid(&pid_b, &ref_b, 20_000, "USD")),
    ]);
    let worker = worker::AutoTopUpWorker::new(wallet.clone(), provider, Duration::from_secs(60));
    worker.reconcile().await.expect("reconcile must not error");

    assert_eq!(fetch_attempt(&pool, id_a).await.0, "SUCCEEDED");
    assert_eq!(fetch_attempt(&pool, id_b).await.0, "SUCCEEDED");
    assert_eq!(
        wallet.find(tenant_id).await.unwrap().unwrap().balance_units,
        30_000
    );
}

#[tokio::test]
#[serial]
async fn reconcile_skips_attempt_without_provider_reference() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_and_wallet(&pool, 0).await;
    let attempt_id = Uuid::new_v4();

    // provider_reference is NULL — `list_auto_top_up_reconciliation_attempts`
    // filters these out before reconcile even sees them.
    sqlx::query(
        "INSERT INTO wallet_auto_top_up_attempts
          (id, tenant_id, amount_units, status, provider_payment_id,
           provider_reference, error_message, attempted_at)
         VALUES ($1, $2, 10_000, 'REQUIRES_RECONCILIATION', NULL, NULL, NULL, NOW())",
    )
    .bind(attempt_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("insert reference-less attempt");

    let provider = provider_with(vec![]);
    let worker = worker::AutoTopUpWorker::new(wallet.clone(), provider, Duration::from_secs(60));
    worker.reconcile().await.expect("reconcile must not error");

    assert_eq!(
        fetch_attempt(&pool, attempt_id).await.0,
        "REQUIRES_RECONCILIATION"
    );
    assert_eq!(
        wallet.find(tenant_id).await.unwrap().unwrap().balance_units,
        0
    );
}
