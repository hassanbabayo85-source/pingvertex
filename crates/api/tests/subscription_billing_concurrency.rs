//! Task 9D.1 — concurrency safety at the real PostgreSQL boundary.
//!
//! These tests stress the UNIQUE constraints that give the webhook and
//! verify paths their idempotency:
//!
//!   * `subscription_billing_records (provider, provider_reference)` (migration 0129)
//!   * `subscriptions.tenant_id` (single active subscription per tenant)
//!
//! Each test fires N parallel operations against a fresh PostgreSQL test
//! schema (from `infra::test_support::setup_pool_exclusive`) and asserts
//! the invariant holds exactly once. Without a real Postgres, the UNIQUE
//! constraint would not be enforced and the test would not be meaningful.
//!
//! Requires `DATABASE_URL` (or the default `postgres://postgres:postgres@localhost:5432/uptime_monitor`)
//! pointing at a reachable PostgreSQL that allows schema creation and
//! `pgcrypto`. This matches the assumption already in
//! `crates/worker/tests/escalation_maintenance_concurrency.rs`.

mod common;

use std::sync::Arc;

use api::error::ApiError;

use api::handlers::paystack_webhook::paystack_webhook;
use api::handlers::subscription_billing::verify_subscription_checkout;
use api::state::AppState;
use application::ports::{
    CheckoutDetails, CheckoutRequest, CheckoutSessionRepository, PaymentProvider,
    PaymentProviderResult, PaymentVerification, SubscriptionCheckoutRequest,
    SubscriptionPaymentVerification,
};
use async_trait::async_trait;
use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode};
use domain::CheckoutSession;
use domain::enums::PlanTier;
use domain::money::{BillingInterval, Currency};
use domain::shared::ids::{CheckoutSessionId, TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use hmac::{Hmac, Mac};
use infra::test_support::{ExclusiveTestGuard, setup_pool_exclusive};
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    NoopNotificationService, PostgresCheckoutSessionRepository,
    PostgresSubscriptionActivationRepository, PostgresSubscriptionBillingRecordRepository,
    PostgresSubscriptionRepository, UuidIdGenerator,
};
use serde_json::json;
use sha2::Sha512;
use sqlx::PgPool;

const WEBHOOK_SECRET: &str = "sk_test_9d_concurrency_audit";
const STARTER_NGN_MONTHLY_KOBO: i64 = 800_000; // ₦8,000 from migration 0125

// ────────────────────────────────────────────────────────────────
// Inline provider — subscription path only
// ────────────────────────────────────────────────────────────────

struct InlineProvider;

#[async_trait]
impl PaymentProvider for InlineProvider {
    fn provider_name(&self) -> &'static str {
        "paystack"
    }
    async fn create_checkout(
        &self,
        _request: &CheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        Err(std::io::Error::other("not used in concurrency tests").into())
    }
    async fn verify_payment(&self, _reference: &str) -> PaymentProviderResult<PaymentVerification> {
        Err(std::io::Error::other("not used in concurrency tests").into())
    }
    async fn create_subscription_checkout(
        &self,
        _request: &SubscriptionCheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        Err(std::io::Error::other("not used in concurrency tests").into())
    }
    async fn verify_payment_minor_units(
        &self,
        reference: &str,
    ) -> PaymentProviderResult<SubscriptionPaymentVerification> {
        Ok(SubscriptionPaymentVerification::Paid {
            provider_payment_id: format!("pay-{reference}"),
            provider_reference: reference.to_owned(),
            amount_minor: STARTER_NGN_MONTHLY_KOBO,
            currency: Currency::Ngn,
        })
    }
}

// ────────────────────────────────────────────────────────────────
// Setup helpers
// ────────────────────────────────────────────────────────────────

async fn setup_pg() -> (PgPool, ExclusiveTestGuard) {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/uptime_monitor".to_string()
    });
    let (pool, guard) = setup_pool_exclusive(&url).await;

    // Sanity: migration 0129 must have created the table we depend on.
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM subscription_billing_records")
        .fetch_one(&pool)
        .await
        .expect("subscription_billing_records must exist (migration 0129)");

    (pool, guard)
}

async fn seed_tenant(pool: &PgPool) -> TenantId {
    let user_id = UserId::new();
    let tenant_id = TenantId::new();

    sqlx::query(
        "INSERT INTO users (id, email, password_hash, full_name, created_at, updated_at) \
         VALUES ($1, $2, 'not-a-real-hash', 'Concurrency Test', NOW(), NOW()) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id.0)
    .bind(format!("{}@concurrency.test", user_id.0))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO tenants (id, name, owner_user_id, created_at) \
         VALUES ($1, $2, $3, NOW()) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id.0)
    .bind(format!("Tenant {}", tenant_id.0))
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, invited_at, joined_at) \
         VALUES ($1, $2, 'Owner', NOW(), NOW()) \
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant_id.0)
    .bind(user_id.0)
    .execute(pool)
    .await
    .unwrap();

    tenant_id
}

async fn seed_checkout(
    checkouts: &PostgresCheckoutSessionRepository,
    tenant: TenantId,
    reference: &str,
) -> CheckoutSessionId {
    let mut session = CheckoutSession::new_pending(
        CheckoutSessionId::new(),
        tenant,
        Some(PlanTier::Starter),
        "paystack".to_owned(),
    );
    session.attach_subscription_billing(
        BillingInterval::Monthly,
        STARTER_NGN_MONTHLY_KOBO,
        Currency::Ngn,
    );
    session.attach_provider_checkout_id(reference.to_owned());
    let id = session.id;
    checkouts.create(&session).await.unwrap();
    id
}

fn build_state(pool: &PgPool) -> AppState {
    let monitor_repo = InMemoryMonitorRepository::new();

    AppState::new(
        monitor_repo.clone(),
        common::NoopCheckRepository,
        common::NoopDiagnosticReportRepository,
        common::NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repo),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(common::NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    )
    .with_checkout_session_repository(PostgresCheckoutSessionRepository::new(pool.clone()))
    .with_subscription_repository(PostgresSubscriptionRepository::new(pool.clone()))
    .with_subscription_billing_record_repository(PostgresSubscriptionBillingRecordRepository::new(
        pool.clone(),
    ))
    .with_subscription_activation_repository(PostgresSubscriptionActivationRepository::new(
        pool.clone(),
    ))
    .with_payment_provider(Arc::new(InlineProvider))
    .with_billing_webhook_secret(WEBHOOK_SECRET)
}

fn sign_webhook(secret: &str, body: &[u8]) -> String {
    let mut mac = <Hmac<Sha512>>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

async fn count_subs(pool: &PgPool, tenant: TenantId) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM subscriptions WHERE tenant_id = $1")
        .bind(tenant.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_succeeded_records(pool: &PgPool, tenant: TenantId) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM subscription_billing_records \
         WHERE tenant_id = $1 AND status = 'succeeded'",
    )
    .bind(tenant.0)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ────────────────────────────────────────────────────────────────
// Test 1: concurrent webhook deliveries activate exactly once
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_webhook_deliveries_activate_subscription_exactly_once() {
    const N: usize = 10;

    let (pool, _guard) = setup_pg().await;
    let tenant = seed_tenant(&pool).await;
    let checkout_repo = PostgresCheckoutSessionRepository::new(pool.clone());
    let _session_id = seed_checkout(&checkout_repo, tenant, "pv-sub-concurrent-1").await;
    let state = build_state(&pool);

    let body = json!({
        "event": "charge.success",
        "data": {
            "reference": "pv-sub-concurrent-1",
            "id": 1,
            "amount": STARTER_NGN_MONTHLY_KOBO,
            "currency": "NGN"
        }
    });
    let body_bytes = Bytes::from(serde_json::to_vec(&body).unwrap());
    let sig = sign_webhook(WEBHOOK_SECRET, &body_bytes);

    let mut handles = Vec::with_capacity(N);
    for _ in 0..N {
        let state = state.clone();
        let body_bytes = body_bytes.clone();
        let sig = sig.clone();
        handles.push(tokio::spawn(async move {
            let mut headers = HeaderMap::new();
            headers.insert("x-paystack-signature", sig.parse().unwrap());
            paystack_webhook(State(state), headers, body_bytes).await
        }));
    }

    let mut ok_count = 0usize;
    let mut completed = 0usize;
    for h in handles {
        let (status, axum::Json(resp)) = h
            .await
            .unwrap()
            .expect("webhook must not return ApiError on a valid signed payload");
        assert_eq!(status, StatusCode::OK);
        ok_count += 1;
        if resp.status == "subscription_completed" {
            completed += 1;
        }
    }

    assert_eq!(ok_count, N, "all webhook calls must complete (not panic)");
    assert_eq!(
        completed, N,
        "every concurrent delivery must resolve to subscription_completed \
         (either wins activation or sees the session already Completed)"
    );

    assert_eq!(
        count_subs(&pool, tenant).await,
        1,
        "exactly one subscription row must exist"
    );
    assert_eq!(
        count_succeeded_records(&pool, tenant).await,
        1,
        "exactly one succeeded billing record must exist \
         (UNIQUE (provider, provider_reference) held under concurrency)"
    );

    // The session must have been marked Completed.
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM checkout_sessions WHERE tenant_id = $1")
            .bind(tenant.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "completed");
}

// ────────────────────────────────────────────────────────────────
// Test 2: concurrent verify handler calls activate exactly once
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_verify_attempts_activate_subscription_exactly_once() {
    const N: usize = 10;

    let (pool, _guard) = setup_pg().await;
    let tenant = seed_tenant(&pool).await;
    let checkout_repo = PostgresCheckoutSessionRepository::new(pool.clone());
    let session_id = seed_checkout(&checkout_repo, tenant, "pv-sub-concurrent-2").await;
    let state = build_state(&pool);

    let user = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant));
    let path = session_id.0.to_string();

    let mut handles = Vec::with_capacity(N);
    for _ in 0..N {
        let state = state.clone();
        let user = user;
        let path = path.clone();
        handles.push(tokio::spawn(async move {
            verify_subscription_checkout(State(state), Extension(user), Path(path)).await
        }));
    }

    let mut completed = 0usize;
    for h in handles {
        let resp = h.await.unwrap().expect("verify must succeed");
        if resp.0.checkout_status == "completed" {
            completed += 1;
        }
    }

    assert_eq!(
        completed, N,
        "all concurrent verifies must resolve to Completed"
    );

    assert_eq!(
        count_subs(&pool, tenant).await,
        1,
        "exactly one subscription row must exist"
    );
    assert_eq!(
        count_succeeded_records(&pool, tenant).await,
        1,
        "exactly one succeeded billing record must exist"
    );
}

// ────────────────────────────────────────────────────────────────
// Test 3: concurrent cross-tenant verifies stay isolated
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_verify_across_tenants_does_not_cross_contaminate() {
    const PER_TENANT: usize = 4;
    const TENANTS: usize = 3;

    let (pool, _guard) = setup_pg().await;
    let checkout_repo = PostgresCheckoutSessionRepository::new(pool.clone());

    let mut sessions = Vec::with_capacity(TENANTS);
    for i in 0..TENANTS {
        let tenant = seed_tenant(&pool).await;
        let reference = format!("pv-sub-iso-{i}");
        let session_id = seed_checkout(&checkout_repo, tenant, &reference).await;
        sessions.push((tenant, session_id));
    }

    let state = build_state(&pool);

    let mut handles = Vec::new();
    for (tenant, session_id) in &sessions {
        let user = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(*tenant));
        for _ in 0..PER_TENANT {
            let state = state.clone();
            let user = user;
            let path = session_id.0.to_string();
            handles.push(tokio::spawn(async move {
                verify_subscription_checkout(State(state), Extension(user), Path(path)).await
            }));
        }
    }

    for h in handles {
        let _ = h
            .await
            .unwrap()
            .expect("every per-tenant verify must succeed");
    }

    for (tenant, _) in &sessions {
        assert_eq!(
            count_subs(&pool, *tenant).await,
            1,
            "tenant {:?} must have exactly one subscription (no cross-tenant leak)",
            tenant
        );
        assert_eq!(
            count_succeeded_records(&pool, *tenant).await,
            1,
            "tenant {:?} must have exactly one succeeded billing record",
            tenant
        );
    }

    // Additional invariant: every subscription belongs to the tenant whose
    // session it originated from — no `subscription.tenant_id` mismatch.
    let mismatched: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM subscriptions s \
         JOIN subscription_billing_records r ON r.subscription_id = s.id \
         WHERE r.tenant_id <> s.tenant_id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        mismatched, 0,
        "no billing record may reference a subscription owned by a different tenant"
    );
}

// ─── BE-1 F3 — real-Postgres crash recovery for Pending records ──

#[tokio::test]
async fn pending_billing_record_is_recovered_by_retry() {
    let (pool, _guard) = setup_pg().await;
    let tenant = seed_tenant(&pool).await;
    let checkout_repo = PostgresCheckoutSessionRepository::new(pool.clone());
    let session_id = seed_checkout(&checkout_repo, tenant, "pv-sub-recovery-1").await;
    let state = build_state(&pool);

    // Simulate a crash that occurred after try_insert but before
    // subscription creation: a Pending billing record exists, no
    // subscriptions row, session Pending.
    sqlx::query(
        "INSERT INTO subscription_billing_records \
         (id, subscription_id, tenant_id, plan_tier, billing_interval, currency, \
          amount_minor, status, provider, provider_reference, provider_payment_id, \
          provider_subscription_code, period_start, period_end, created_at, updated_at) \
         VALUES ($1, $2, $3, 'starter', 'monthly', 'NGN', $4, 'pending', \
                 'paystack', 'pv-sub-recovery-1', 'pay-pre-crash', NULL, \
                 now(), now() + interval '30 days', now(), now())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(uuid::Uuid::new_v4())
    .bind(tenant.0)
    .bind(STARTER_NGN_MONTHLY_KOBO)
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(count_subs(&pool, tenant).await, 0);
    assert_eq!(count_succeeded_records(&pool, tenant).await, 0);

    // Trigger retry through the handler.
    let user = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant));
    let path = session_id.0.to_string();
    let resp = verify_subscription_checkout(State(state), Extension(user), Path(path))
        .await
        .unwrap();
    assert_eq!(resp.0.checkout_status, "completed");

    // Post-state: subscription created, record Succeeded, session Completed.
    assert_eq!(count_subs(&pool, tenant).await, 1);
    assert_eq!(count_succeeded_records(&pool, tenant).await, 1);

    let sub_status: String =
        sqlx::query_scalar("SELECT status FROM subscriptions WHERE tenant_id = $1")
            .bind(tenant.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sub_status, "active");
}

// ─── BE-1 F5 — atomicity, rollback, terminal-subscription rejection ──

/// Rollback test: Step 6 (session completion) fails because the session
/// was previously marked `failed`. The port must roll back the record
/// insert (Step 1) and the subscription insert (Step 4) so nothing is
/// left half-done.
#[tokio::test]
async fn rollback_leaves_no_partial_state_on_session_update_failure() {
    let (pool, _guard) = setup_pg().await;
    let tenant = seed_tenant(&pool).await;
    let checkout_repo = PostgresCheckoutSessionRepository::new(pool.clone());
    let session_id = seed_checkout(&checkout_repo, tenant, "pv-sub-rollback-1").await;
    let state = build_state(&pool);

    // Mark the session `failed` to force Step 6's `status <> 'failed'`
    // guard to reject the update.
    sqlx::query("UPDATE checkout_sessions SET status = 'failed' WHERE id = $1")
        .bind(session_id.0)
        .execute(&pool)
        .await
        .unwrap();

    let user = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant));
    let path = session_id.0.to_string();
    let err = verify_subscription_checkout(State(state), Extension(user), Path(path))
        .await
        .unwrap_err();

    assert!(
        matches!(&err, ApiError::BadRequest(m)
            if m.contains("checkout session could not be marked completed")),
        "expected BadRequest(step-6), got {err:?}"
    );

    // Rollback: no billing record, no subscription.
    let records: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM subscription_billing_records WHERE tenant_id = $1",
    )
    .bind(tenant.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(records, 0, "billing record must be rolled back");
    assert_eq!(
        count_subs(&pool, tenant).await,
        0,
        "subscription must be rolled back"
    );

    let session_status: String =
        sqlx::query_scalar("SELECT status FROM checkout_sessions WHERE id = $1")
            .bind(session_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "failed", "session status unchanged");
}

/// Succeeded record pointing at a Canceled subscription: port must
/// reject (400-class) and leave the session `pending`.
#[tokio::test]
async fn succeeded_record_with_canceled_subscription_returns_client_error() {
    let (pool, _guard) = setup_pg().await;
    let tenant = seed_tenant(&pool).await;
    let checkout_repo = PostgresCheckoutSessionRepository::new(pool.clone());
    let session_id = seed_checkout(&checkout_repo, tenant, "pv-sub-canceled-1").await;
    let state = build_state(&pool);

    let sub_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO subscriptions (id, tenant_id, plan_tier, status, created_at, updated_at) \
         VALUES ($1, $2, 'starter', 'canceled', now(), now())",
    )
    .bind(sub_id)
    .bind(tenant.0)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO subscription_billing_records \
         (id, subscription_id, tenant_id, plan_tier, billing_interval, currency, \
          amount_minor, status, provider, provider_reference, provider_payment_id, \
          provider_subscription_code, period_start, period_end, created_at, updated_at) \
         VALUES ($1, $2, $3, 'starter', 'monthly', 'NGN', $4, 'succeeded', \
                 'paystack', 'pv-sub-canceled-1', 'pay-pre', NULL, \
                 now(), now() + interval '30 days', now(), now())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(sub_id)
    .bind(tenant.0)
    .bind(STARTER_NGN_MONTHLY_KOBO)
    .execute(&pool)
    .await
    .unwrap();

    let user = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant));
    let path = session_id.0.to_string();
    let err = verify_subscription_checkout(State(state), Extension(user), Path(path))
        .await
        .unwrap_err();

    // BE-1 F5 — the use case's domain transition
    // (`apply_verified_payment_with_period`) rejects Canceled subscriptions
    // BEFORE the port is reached, so the handler returns `Conflict` (409)
    // with a domain-transition message — not `BadRequest` from the port's
    // `SucceededSubscriptionTerminal` guard. Both are 4xx rejections and
    // both correctly refuse reactivation.
    assert!(
        matches!(&err, ApiError::Conflict(m)
            if m.contains("canceled subscription")),
        "expected Conflict(domain-level rejection of canceled subscription), got {err:?}"
    );

    let session_status: String =
        sqlx::query_scalar("SELECT status FROM checkout_sessions WHERE id = $1")
            .bind(session_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "pending");
}

/// Succeeded record pointing at an Expired subscription: same as above.
#[tokio::test]
async fn succeeded_record_with_expired_subscription_returns_client_error() {
    let (pool, _guard) = setup_pg().await;
    let tenant = seed_tenant(&pool).await;
    let checkout_repo = PostgresCheckoutSessionRepository::new(pool.clone());
    let session_id = seed_checkout(&checkout_repo, tenant, "pv-sub-expired-1").await;
    let state = build_state(&pool);

    let sub_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO subscriptions (id, tenant_id, plan_tier, status, created_at, updated_at) \
         VALUES ($1, $2, 'starter', 'expired', now(), now())",
    )
    .bind(sub_id)
    .bind(tenant.0)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO subscription_billing_records \
         (id, subscription_id, tenant_id, plan_tier, billing_interval, currency, \
          amount_minor, status, provider, provider_reference, provider_payment_id, \
          provider_subscription_code, period_start, period_end, created_at, updated_at) \
         VALUES ($1, $2, $3, 'starter', 'monthly', 'NGN', $4, 'succeeded', \
                 'paystack', 'pv-sub-expired-1', 'pay-pre', NULL, \
                 now(), now() + interval '30 days', now(), now())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(sub_id)
    .bind(tenant.0)
    .bind(STARTER_NGN_MONTHLY_KOBO)
    .execute(&pool)
    .await
    .unwrap();

    let user = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant));
    let path = session_id.0.to_string();
    let err = verify_subscription_checkout(State(state), Extension(user), Path(path))
        .await
        .unwrap_err();

    assert!(
        matches!(&err, ApiError::BadRequest(m) if m.contains("terminal state")),
        "expected BadRequest(terminal), got {err:?}"
    );
}

/// Pending record whose `subscription_id` in the seed was wrong: the port
/// must correct it to the actual persisted subscription id (Step 4b).
#[tokio::test]
async fn pending_record_subscription_id_is_relinked_to_persisted_row() {
    let (pool, _guard) = setup_pg().await;
    let tenant = seed_tenant(&pool).await;
    let checkout_repo = PostgresCheckoutSessionRepository::new(pool.clone());
    let session_id = seed_checkout(&checkout_repo, tenant, "pv-sub-linkage-1").await;
    let state = build_state(&pool);

    let stale_sub_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO subscription_billing_records \
         (id, subscription_id, tenant_id, plan_tier, billing_interval, currency, \
          amount_minor, status, provider, provider_reference, provider_payment_id, \
          provider_subscription_code, period_start, period_end, created_at, updated_at) \
         VALUES ($1, $2, $3, 'starter', 'monthly', 'NGN', $4, 'pending', \
                 'paystack', 'pv-sub-linkage-1', 'pay-pre', NULL, \
                 now(), now() + interval '30 days', now(), now())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(stale_sub_id)
    .bind(tenant.0)
    .bind(STARTER_NGN_MONTHLY_KOBO)
    .execute(&pool)
    .await
    .unwrap();

    let user = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant));
    let path = session_id.0.to_string();
    let resp = verify_subscription_checkout(State(state), Extension(user), Path(path))
        .await
        .unwrap();
    assert_eq!(resp.0.checkout_status, "completed");

    let rec_sub: uuid::Uuid = sqlx::query_scalar(
        "SELECT subscription_id FROM subscription_billing_records \
         WHERE provider = 'paystack' AND provider_reference = 'pv-sub-linkage-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let actual_sub: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM subscriptions WHERE tenant_id = $1")
            .bind(tenant.0)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_ne!(rec_sub, stale_sub_id, "stale id must be replaced");
    assert_eq!(
        rec_sub, actual_sub,
        "Step 4b must relink record to the actual persisted subscription"
    );
}
