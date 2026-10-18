//! Task 6 — Paystack webhook handler tests for subscription vs wallet.
//!
//! Focuses on the branching logic in `handle_charge_success`:
//!   * `pv-sub-*` references route to subscription activation and,
//!     after server-side verification, create an Active subscription.
//!   * `pv-wallet-*` references still route to the wallet path — a
//!     non-existent wallet session returns `unknown_reference` without
//!     touching subscriptions. This is the wallet regression.
//!   * Duplicate subscription webhooks do not double-activate.
//!   * Forged signatures are rejected before any state change.

mod common;

use std::sync::Arc;

use api::handlers::paystack_webhook::paystack_webhook;
use api::state::AppState;
use application::ports::{
    CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentProviderResult, PaymentVerification,
    SubscriptionCheckoutRequest, SubscriptionPaymentVerification, SubscriptionRepository,
};
use async_trait::async_trait;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use domain::CheckoutSession;
use domain::enums::{PlanTier, SubscriptionStatus};
use domain::money::{BillingInterval, Currency};
use domain::shared::ids::{CheckoutSessionId, TenantId};
use hmac::{Hmac, Mac};
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryCheckoutSessionRepository,
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, InMemorySubscriptionBillingRecordRepository,
    InMemorySubscriptionRepository, NoopNotificationService, UuidIdGenerator,
};
use serde_json::json;
use sha2::Sha512;

const WEBHOOK_SECRET: &str = "sk_test_task6_webhook_secret";

// ── Fake provider ─────────────────────────────────────────────────────

struct FakeProvider {
    verify_amount_minor: i64,
    verify_currency: Currency,
}

#[async_trait]
impl PaymentProvider for FakeProvider {
    fn provider_name(&self) -> &'static str {
        "paystack"
    }

    async fn create_checkout(
        &self,
        _request: &CheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        Err(std::io::Error::other("wallet checkout not used").into())
    }

    async fn verify_payment(
        &self,
        _reference: &str,
    ) -> PaymentProviderResult<PaymentVerification> {
        Err(std::io::Error::other("wallet verify not used").into())
    }

    async fn create_subscription_checkout(
        &self,
        _request: &SubscriptionCheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        Err(std::io::Error::other("not used in webhook test").into())
    }

    async fn verify_payment_minor_units(
        &self,
        _reference: &str,
    ) -> PaymentProviderResult<SubscriptionPaymentVerification> {
        Ok(SubscriptionPaymentVerification::Paid {
            provider_payment_id: "pay-webhook-1".to_owned(),
            provider_reference: "ref-webhook-1".to_owned(),
            amount_minor: self.verify_amount_minor,
            currency: self.verify_currency,
        })
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = <Hmac<Sha512>>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

struct Fixture {
    state: AppState,
    tenant_id: TenantId,
    subscription_repo: Arc<InMemorySubscriptionRepository>,
}

fn build_fixture(verify_amount_minor: i64, verify_currency: Currency) -> Fixture {
    let monitor_repo = InMemoryMonitorRepository::new();
    let sub_repo = Arc::new(InMemorySubscriptionRepository::new());
    let checkout_repo = Arc::new(InMemoryCheckoutSessionRepository::new());
    let records_repo = Arc::new(InMemorySubscriptionBillingRecordRepository::new());

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
    .with_checkout_session_repository(checkout_repo)
    .with_subscription_repository(sub_repo.clone())
    .with_subscription_billing_record_repository(records_repo)
    .with_payment_provider(FakeProvider {
        verify_amount_minor,
        verify_currency,
    })
    .with_billing_webhook_secret(WEBHOOK_SECRET);

    Fixture {
        state,
        tenant_id: TenantId(uuid::Uuid::new_v4()),
        subscription_repo: sub_repo,
    }
}

async fn seed_subscription_session(
    state: &AppState,
    tenant_id: TenantId,
    reference: &str,
    amount_minor: i64,
    currency: Currency,
) {
    let mut session = CheckoutSession::new_pending(
        CheckoutSessionId::new(),
        tenant_id,
        Some(PlanTier::Starter),
        "paystack".to_owned(),
    );
    session.attach_subscription_billing(BillingInterval::Monthly, amount_minor, currency);
    session.attach_provider_checkout_id(reference.to_owned());
    state
        .checkout_session_repository
        .create(&session)
        .await
        .unwrap();
}

async fn post_webhook(state: AppState, body: serde_json::Value) -> (StatusCode, String) {
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-paystack-signature",
        sign(WEBHOOK_SECRET, &body_bytes).parse().unwrap(),
    );
    let (status, axum::Json(response)) = paystack_webhook(
        State(state),
        headers,
        Bytes::from(body_bytes),
    )
    .await
    .unwrap();
    (status, response.status)
}

// ── Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn subscription_charge_success_activates_subscription() {
    let f = build_fixture(800_000, Currency::Ngn);
    seed_subscription_session(
        &f.state,
        f.tenant_id,
        "pv-sub-webhook-1",
        800_000,
        Currency::Ngn,
    )
    .await;

    let (status, result) = post_webhook(
        f.state.clone(),
        json!({
            "event": "charge.success",
            "data": {
                "reference": "pv-sub-webhook-1",
                "id": 999,
                "amount": 800_000,
                "currency": "NGN"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, "subscription_completed");

    let sub = f
        .subscription_repo
        .find_by_tenant_id(f.tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sub.plan_tier, PlanTier::Starter);
    assert_eq!(sub.status, SubscriptionStatus::Active);
    assert!(sub.current_period_ends_at.is_some());
}

#[tokio::test]
async fn duplicate_subscription_webhook_does_not_double_activate() {
    let f = build_fixture(800_000, Currency::Ngn);
    seed_subscription_session(
        &f.state,
        f.tenant_id,
        "pv-sub-dup-1",
        800_000,
        Currency::Ngn,
    )
    .await;

    let body = json!({
        "event": "charge.success",
        "data": {"reference": "pv-sub-dup-1", "id": 1, "amount": 800_000, "currency": "NGN"}
    });

    let (_, first) = post_webhook(f.state.clone(), body.clone()).await;
    assert_eq!(first, "subscription_completed");
    let sub_after_first = f
        .subscription_repo
        .find_by_tenant_id(f.tenant_id)
        .await
        .unwrap()
        .unwrap();

    let (_, second) = post_webhook(f.state.clone(), body).await;
    assert_eq!(second, "subscription_completed");
    let sub_after_second = f
        .subscription_repo
        .find_by_tenant_id(f.tenant_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(sub_after_first.id, sub_after_second.id);
    assert_eq!(
        sub_after_first.current_period_ends_at,
        sub_after_second.current_period_ends_at,
        "duplicate webhook must not extend the period"
    );
}

#[tokio::test]
async fn wallet_reference_still_routes_to_wallet_path() {
    let f = build_fixture(800_000, Currency::Ngn);

    let (status, result) = post_webhook(
        f.state.clone(),
        json!({
            "event": "charge.success",
            "data": {"reference": "pv-wallet-does-not-exist", "id": 42, "amount": 1000, "currency": "USD"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    // Unknown wallet reference — the wallet path was taken, not subscription.
    assert_eq!(result, "unknown_reference");
    // No subscription was created.
    assert!(
        f.subscription_repo
            .find_by_tenant_id(f.tenant_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn forged_signature_is_rejected_before_any_state_change() {
    let f = build_fixture(800_000, Currency::Ngn);
    seed_subscription_session(
        &f.state,
        f.tenant_id,
        "pv-sub-forged",
        800_000,
        Currency::Ngn,
    )
    .await;

    // Post an UNSIGNED body.
    let body_bytes = serde_json::to_vec(&json!({
        "event": "charge.success",
        "data": {"reference": "pv-sub-forged", "id": 1, "amount": 800_000, "currency": "NGN"}
    }))
    .unwrap();

    let result = paystack_webhook(
        State(f.state.clone()),
        HeaderMap::new(),
        Bytes::from(body_bytes),
    )
    .await;

    assert!(result.is_err(), "unsigned webhook must be rejected");
    assert!(
        f.subscription_repo
            .find_by_tenant_id(f.tenant_id)
            .await
            .unwrap()
            .is_none(),
        "no subscription may be activated on a forged webhook"
    );
}

#[tokio::test]
async fn subscription_webhook_for_unknown_reference_is_ignored() {
    let f = build_fixture(800_000, Currency::Ngn);
    // No session seeded for "pv-sub-ghost"

    let (status, result) = post_webhook(
        f.state.clone(),
        json!({
            "event": "charge.success",
            "data": {"reference": "pv-sub-ghost", "id": 1, "amount": 800_000, "currency": "NGN"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, "unknown_reference");
    assert!(
        f.subscription_repo
            .find_by_tenant_id(f.tenant_id)
            .await
            .unwrap()
            .is_none()
    );
}
