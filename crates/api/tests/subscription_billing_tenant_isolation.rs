//! Task 9B-4 — cross-tenant isolation for subscription billing handlers.
//!
//! Verifies that Tenant A cannot read, verify, or cancel a subscription
//! or checkout session owned by Tenant B. Every assertion goes through
//! the real handler boundary, so it exercises the production auth +
//! permission + tenant checks (not helper functions).
//!
//! Security invariant: cross-tenant access returns 404 (NotFound), not
//! 403, so a foreign UUID cannot be confirmed to exist.

mod common;

use std::sync::Arc;

use api::error::ApiError;
use api::handlers::subscription_billing::{
    CancelSubscriptionBody, cancel_subscription_endpoint, get_subscription,
    verify_subscription_checkout,
};
use api::state::AppState;
use application::ports::{CheckoutSessionRepository, SubscriptionRepository};
use axum::Json;
use axum::extract::{Extension, Path, State};
use chrono::{Duration, Utc};
use domain::enums::PlanTier;
use domain::money::{BillingInterval, Currency};
use domain::shared::ids::{CheckoutSessionId, SubscriptionId, TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use domain::{CheckoutSession, Subscription, SubscriptionStatus};
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryCheckoutSessionRepository,
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryPlanPriceRepository, InMemoryProxyCheckJobRepository,
    InMemorySubscriptionBillingRecordRepository, InMemorySubscriptionRepository,
    NoopNotificationService, UuidIdGenerator,
};

struct Fixture {
    state: AppState,
    checkouts: Arc<InMemoryCheckoutSessionRepository>,
    subscriptions: Arc<InMemorySubscriptionRepository>,
    tenant_a: TenantId,
    tenant_b: TenantId,
    user_a: AuthenticatedUser,
    user_b: AuthenticatedUser,
}

fn build_fixture() -> Fixture {
    let monitor_repo = InMemoryMonitorRepository::new();
    let checkouts = Arc::new(InMemoryCheckoutSessionRepository::new());
    let subscriptions = Arc::new(InMemorySubscriptionRepository::new());

    let state = AppState::new(
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
    .with_plan_price_repository(InMemoryPlanPriceRepository::default())
    .with_checkout_session_repository(checkouts.clone())
    .with_subscription_repository(subscriptions.clone())
    .with_subscription_billing_record_repository(
        InMemorySubscriptionBillingRecordRepository::new(),
    );

    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();

    // Owner has ManageBilling + ViewBilling, so the permission gate passes
    // and the tenant check is what actually blocks cross-tenant access.
    let user_a = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant_a));
    let user_b = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant_b));

    Fixture {
        state,
        checkouts,
        subscriptions,
        tenant_a,
        tenant_b,
        user_a,
        user_b,
    }
}

async fn seed_subscription(
    subscriptions: &InMemorySubscriptionRepository,
    tenant: TenantId,
    plan: PlanTier,
) -> Subscription {
    let sub = Subscription::new_with_period(
        SubscriptionId::new(),
        tenant,
        plan,
        Utc::now() + Duration::days(30),
    );
    subscriptions.create(&sub).await.unwrap();
    sub
}

async fn seed_checkout_session(
    checkouts: &InMemoryCheckoutSessionRepository,
    tenant: TenantId,
    reference: &str,
) -> CheckoutSessionId {
    let mut session = CheckoutSession::new_pending(
        CheckoutSessionId::new(),
        tenant,
        Some(PlanTier::Starter),
        "paystack".to_owned(),
    );
    session.attach_subscription_billing(BillingInterval::Monthly, 800_000, Currency::Ngn);
    session.attach_provider_checkout_id(reference.to_owned());
    let id = session.id;
    checkouts.create(&session).await.unwrap();
    id
}

// ────────────────────────────────────────────────────────────────
// GET /billing/subscription
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_subscription_does_not_leak_other_tenants_data() {
    let f = build_fixture();
    // Tenant B has a subscription; Tenant A has none.
    seed_subscription(&f.subscriptions, f.tenant_b, PlanTier::Pro).await;

    let response = get_subscription(State(f.state.clone()), Extension(f.user_a))
        .await
        .expect("Tenant A reading its own (empty) subscription must succeed");

    assert!(
        response.0.subscription.is_none(),
        "Tenant A must not see Tenant B's subscription — got {:?}",
        response.0.subscription
    );
}

#[tokio::test]
async fn get_subscription_is_scoped_per_tenant() {
    let f = build_fixture();
    seed_subscription(&f.subscriptions, f.tenant_a, PlanTier::Starter).await;
    seed_subscription(&f.subscriptions, f.tenant_b, PlanTier::Business).await;

    let a = get_subscription(State(f.state.clone()), Extension(f.user_a))
        .await
        .expect("A read")
        .0;
    let b = get_subscription(State(f.state.clone()), Extension(f.user_b))
        .await
        .expect("B read")
        .0;

    assert_eq!(
        a.subscription.as_ref().map(|s| s.plan_tier.as_str()),
        Some("starter"),
        "Tenant A must see only its own plan"
    );
    assert_eq!(
        b.subscription.as_ref().map(|s| s.plan_tier.as_str()),
        Some("business"),
        "Tenant B must see only its own plan"
    );
}

// ────────────────────────────────────────────────────────────────
// POST /billing/subscription/checkout/{id}/verify
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn verify_checkout_rejects_cross_tenant_session() {
    let f = build_fixture();
    let b_session = seed_checkout_session(&f.checkouts, f.tenant_b, "pv-sub-iso-1").await;
    let session_id = b_session.0.to_string();

    let err = verify_subscription_checkout(
        State(f.state.clone()),
        Extension(f.user_a),
        Path(session_id),
    )
    .await
    .expect_err("Tenant A must not verify Tenant B's checkout session");

    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected 404 (no existence leak), got {err:?}"
    );

    // Tenant B's session is still pending (Tenant A's attempt did nothing).
    let b_after = f
        .checkouts
        .find_by_id(b_session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        b_after.status,
        domain::enums::CheckoutStatus::Pending,
        "Tenant B's checkout session must remain pending"
    );
}

// ────────────────────────────────────────────────────────────────
// POST /billing/subscription/cancel
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cancel_subscription_does_not_affect_other_tenant() {
    let f = build_fixture();
    let b_sub = seed_subscription(&f.subscriptions, f.tenant_b, PlanTier::Pro).await;

    let err = cancel_subscription_endpoint(
        State(f.state.clone()),
        Extension(f.user_a),
        Json(CancelSubscriptionBody::default()),
    )
    .await
    .expect_err("Tenant A has no subscription to cancel");

    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected 404, got {err:?}"
    );

    // Tenant B's subscription is untouched.
    let b_after = f
        .subscriptions
        .find_by_tenant_id(f.tenant_b)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b_after.id, b_sub.id);
    assert_eq!(b_after.status, SubscriptionStatus::Active);
    assert!(!b_after.cancel_at_period_end);
}
