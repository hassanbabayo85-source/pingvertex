//! PART 4 — Financial DB immutability tests.
//!
//! Every test inserts a row into a financial table, then attempts
//! UPDATE and DELETE, and asserts the DB raises. These tests prove
//! that the invariants hold at the DATABASE layer, not merely at the
//! application layer.
//!
//! Graceful skip: if `DATABASE_URL` is not set or Postgres is
//! unreachable, each test returns Ok without asserting anything.

use application::ports::{TenantRepository, WalletRepository};
use domain::WalletTransactionType;
use domain::shared::ids::{TenantId, UserId};
use infra::repositories::postgres_tenant_repository::PostgresTenantRepository;
use infra::repositories::postgres_wallet_repository::PostgresWalletRepository;
use sqlx::PgPool;
use std::sync::Arc;
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
         VALUES ($1, $2, 'not-a-real-hash', 'PART4 Test', NOW(), NOW())
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@part4-immutability.test", user_id.0))
    .execute(pool)
    .await
    .expect("insert test user");

    let tenant_repo = PostgresTenantRepository::new(pool.clone());
    let tenant =
        domain::Tenant::new(TenantId::new(), "PART4 Tenant", user_id).expect("valid tenant");
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
// 1) wallet_transactions
// ===========================================================================

#[tokio::test]
async fn wallet_transactions_cannot_be_updated_or_deleted() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 0).await;

    wallet
        .credit(
            tenant_id,
            1_000,
            WalletTransactionType::TopUp,
            "txn-immutability-1",
            None,
        )
        .await
        .unwrap();

    let tx_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM wallet_transactions WHERE tenant_id=$1 AND idempotency_key='txn-immutability-1'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let update = sqlx::query("UPDATE wallet_transactions SET amount_units=999999 WHERE id=$1")
        .bind(tx_id)
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "wallet_transactions UPDATE must be rejected"
    );
    let msg = update.unwrap_err().to_string();
    assert!(
        msg.contains("immutable"),
        "error must mention immutability, got: {msg}"
    );

    let delete = sqlx::query("DELETE FROM wallet_transactions WHERE id=$1")
        .bind(tx_id)
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "wallet_transactions DELETE must be rejected"
    );
}

// ===========================================================================
// 2) api_usage_events
// ===========================================================================

#[tokio::test]
async fn api_usage_events_cannot_be_updated_or_deleted() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, _wallet) = setup_tenant_wallet(&pool, 0).await;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO api_usage_events
         (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,pricing_version,request_id,status)
         VALUES ($1,$2,'http_check.one_time',1,1,1000,1000,'USD',1,$3,'CAPTURED')",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(format!("part4-usage-{}", Uuid::new_v4()))
    .execute(&pool)
    .await
    .unwrap();

    let update = sqlx::query("UPDATE api_usage_events SET charge_units=999999 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(update.is_err(), "api_usage_events UPDATE must be rejected");

    let delete = sqlx::query("DELETE FROM api_usage_events WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(delete.is_err(), "api_usage_events DELETE must be rejected");
}

// ===========================================================================
// 3) financial_audit_log
// ===========================================================================

#[tokio::test]
async fn financial_audit_log_cannot_be_updated_or_deleted() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, _wallet) = setup_tenant_wallet(&pool, 0).await;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO financial_audit_log (id,actor,tenant_id,action,resource,resource_id,metadata)
         VALUES ($1,'part4-test',$2,'part4_action','wallet',$3,'{}'::jsonb)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&pool)
    .await
    .unwrap();

    let update = sqlx::query("UPDATE financial_audit_log SET action='tampered' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "financial_audit_log UPDATE must be rejected"
    );

    let delete = sqlx::query("DELETE FROM financial_audit_log WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "financial_audit_log DELETE must be rejected"
    );
}

// ===========================================================================
// 4) billing_audit_log (NEW trigger in migration 0121)
// ===========================================================================

#[tokio::test]
async fn billing_audit_log_cannot_be_updated_or_deleted() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, _wallet) = setup_tenant_wallet(&pool, 0).await;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO billing_audit_log (id,provider_event_id,event_type,tenant_id,payload,created_at)
         VALUES ($1,$2,'payment_succeeded',$3,'{}'::jsonb,NOW())",
    )
    .bind(id)
    .bind(format!("evt-{}", Uuid::new_v4()))
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let update =
        sqlx::query("UPDATE billing_audit_log SET event_type='payment_failed' WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await;
    assert!(update.is_err(), "billing_audit_log UPDATE must be rejected");

    let delete = sqlx::query("DELETE FROM billing_audit_log WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(delete.is_err(), "billing_audit_log DELETE must be rejected");
}

// ===========================================================================
// 5) pricing_configs (NEW trigger in migration 0121)
// ===========================================================================

#[tokio::test]
async fn pricing_configs_cannot_be_updated_or_deleted() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };

    let id = Uuid::new_v4();
    let version: i32 = 900_000 + (Uuid::new_v4().as_u128() as i32 % 10_000).abs();
    sqlx::query(
        "INSERT INTO pricing_configs (
            id, version, currency, cost_per_check, effective_storage_bytes_per_check,
            cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold, backup_multiplier, markup_multiplier,
            min_interval_seconds, max_monitors, max_retention_days, default_retention_days,
            min_paid_monthly_price, free_monitors, free_interval_seconds, free_retention_days,
            avg_billable_network_bytes_per_check, cost_per_gb_egress, cost_safety_buffer,
            sms_markup_multiplier, enterprise_sla_monthly_price, effective_at)
         VALUES ($1,$2,'USD',0,1024,0,0,0,1,1,30,1000,30,30,0,10,300,3,0,0,1,1,0,NOW())",
    )
    .bind(id)
    .bind(version)
    .execute(&pool)
    .await
    .unwrap();

    let update = sqlx::query("UPDATE pricing_configs SET cost_per_check=999 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(update.is_err(), "pricing_configs UPDATE must be rejected");

    let delete = sqlx::query("DELETE FROM pricing_configs WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(delete.is_err(), "pricing_configs DELETE must be rejected");
}

// ===========================================================================
// 6) payg_pricing_configs
// ===========================================================================

#[tokio::test]
async fn payg_pricing_configs_cannot_be_updated_or_deleted() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };

    let id = Uuid::new_v4();
    let operation = format!("part4.op.{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO payg_pricing_configs
         (id,version,operation,unit_name,unit_price,currency,effective_from,active)
         VALUES ($1,$2,$3,'unit',1000,'USD',NOW(),TRUE)",
    )
    .bind(id)
    .bind(9_000_000_i64 + (Uuid::new_v4().as_u128() as i64 % 10_000))
    .bind(&operation)
    .execute(&pool)
    .await
    .unwrap();

    let update = sqlx::query("UPDATE payg_pricing_configs SET unit_price=999 WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "payg_pricing_configs UPDATE must be rejected"
    );

    let delete = sqlx::query("DELETE FROM payg_pricing_configs WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "payg_pricing_configs DELETE must be rejected"
    );
}

// ===========================================================================
// 7) credit_reservations state machine (NEW trigger in migration 0121)
// ===========================================================================

#[tokio::test]
async fn captured_reservation_status_is_terminal() {
    use chrono::{Duration, Utc};

    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 10_000).await;

    let request_id = format!("part4-res-{}", Uuid::new_v4());
    let outcome = wallet
        .reserve(
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
        .unwrap();

    wallet
        .capture(outcome.reservation.id, None, &[], None)
        .await
        .unwrap();

    let update = sqlx::query("UPDATE credit_reservations SET status='PENDING' WHERE id=$1")
        .bind(outcome.reservation.id)
        .execute(&pool)
        .await;
    assert!(
        update.is_err(),
        "credit_reservations CAPTURED -> PENDING must be rejected by trigger"
    );

    let delete = sqlx::query("DELETE FROM credit_reservations WHERE id=$1")
        .bind(outcome.reservation.id)
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "credit_reservations DELETE must be rejected"
    );
}

// ===========================================================================
// 8) wallet_top_up_sessions state machine (NEW trigger in migration 0121)
// ===========================================================================

#[tokio::test]
async fn completed_top_up_session_cannot_transition_to_failed() {
    let Some((pool, _guard)) = pool_or_skip().await else {
        return;
    };
    let (tenant_id, wallet) = setup_tenant_wallet(&pool, 0).await;

    let checkout_id = format!("chk-{}", Uuid::new_v4());
    let session = domain::WalletTopUpSession {
        id: Uuid::new_v4(),
        tenant_id,
        provider: "paystack".to_string(),
        provider_checkout_id: checkout_id.clone(),
        checkout_url: "https://example.test/pay".to_string(),
        idempotency_key: format!("idem-{}", Uuid::new_v4()),
        provider_payment_id: None,
        amount_units: 5_000,
        refunded_amount_units: 0,
        currency: "USD".to_string(),
        status: "PENDING".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    wallet.create_top_up_session(&session).await.unwrap();
    wallet
        .complete_top_up_session(tenant_id, &checkout_id, "provider-pay-1")
        .await
        .unwrap();

    // A late charge.failed webhook must not be able to rewrite COMPLETED -> FAILED.
    let update = sqlx::query(
        "UPDATE wallet_top_up_sessions SET status='FAILED' WHERE tenant_id=$1 AND provider_checkout_id=$2",
    )
    .bind(tenant_id)
    .bind(&checkout_id)
    .execute(&pool)
    .await;
    assert!(
        update.is_err(),
        "wallet_top_up_sessions COMPLETED -> FAILED must be rejected by trigger"
    );
}
