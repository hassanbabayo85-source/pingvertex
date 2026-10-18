//! Task 6 — Paystack subscription checkout + integer-minor-units verify.
//!
//! Confirms that the subscription path:
//!   * requires email and a positive amount;
//!   * sends NGN amounts in kobo and USD amounts in cents
//!     (no wallet-era ×10_000 conversion);
//!   * returns raw `data.amount` as `amount_minor`;
//!   * accepts NGN (the wallet path rejects it — this path must not);
//!   * maps transaction status to Paid / Pending / Failed.

use application::ports::{
    PaymentProvider, SubscriptionCheckoutRequest, SubscriptionPaymentVerification,
};
use domain::enums::PlanTier;
use domain::money::{BillingInterval, Currency};
use domain::shared::ids::TenantId;
use infra::PaystackBillingProvider;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "sk_test_paystack_subscription";
const AUTH_HEADER: &str = "Bearer sk_test_paystack_subscription";

fn tenant() -> TenantId {
    TenantId(uuid::Uuid::new_v4())
}

async fn provider_for(server: &MockServer) -> PaystackBillingProvider {
    PaystackBillingProvider::with_base_url(SECRET.to_owned(), server.uri()).unwrap()
}

fn sub_request(
    amount_minor: i64,
    currency: Currency,
    interval: BillingInterval,
    email: &str,
    reference: &str,
) -> SubscriptionCheckoutRequest {
    SubscriptionCheckoutRequest {
        tenant_id: tenant(),
        plan_tier: PlanTier::Starter,
        billing_interval: interval,
        amount_minor,
        currency,
        customer_email: email.to_owned(),
        reference: reference.to_owned(),
    }
}

// ============================================================================
// create_subscription_checkout
// ============================================================================

#[tokio::test]
async fn create_subscription_checkout_requires_email_before_any_network_call() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    let err = provider
        .create_subscription_checkout(&sub_request(
            500,
            Currency::Usd,
            BillingInterval::Monthly,
            "   ",
            "pv-sub-test-1",
        ))
        .await
        .unwrap_err();

    assert!(err.to_string().contains("email"));
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "no HTTP call may be made when email is blank"
    );
}

#[tokio::test]
async fn create_subscription_checkout_rejects_non_positive_amount() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    for amount in [0i64, -1, -100] {
        let err = provider
            .create_subscription_checkout(&sub_request(
                amount,
                Currency::Usd,
                BillingInterval::Monthly,
                "alice@example.com",
                "pv-sub-test-2",
            ))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("positive"));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn create_subscription_checkout_sends_ngn_amount_in_kobo() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/initialize"))
        .and(header("authorization", AUTH_HEADER))
        .and(header("content-type", "application/json"))
        .and(body_json(json!({
            "email": "alice@example.com",
            "amount": 800_000,          // ₦8,000 in kobo — NOT ×10_000
            "currency": "NGN",
            "reference": "pv-sub-ngn-1"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "data": {
                "authorization_url": "https://checkout.paystack.com/ngn-1",
                "reference": "pv-sub-ngn-1"
            }
        })))
        .mount(&server)
        .await;

    let details = provider
        .create_subscription_checkout(&sub_request(
            800_000,
            Currency::Ngn,
            BillingInterval::Monthly,
            "alice@example.com",
            "pv-sub-ngn-1",
        ))
        .await
        .unwrap();
    assert_eq!(details.provider_checkout_id, "pv-sub-ngn-1");
}

#[tokio::test]
async fn create_subscription_checkout_sends_usd_annual_amount_in_cents() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/initialize"))
        .and(header("authorization", AUTH_HEADER))
        .and(body_json(json!({
            "email": "alice@example.com",
            "amount": 50_000,           // $500 in cents — NOT ×10_000
            "currency": "USD",
            "reference": "pv-sub-usd-1"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "data": {
                "authorization_url": "https://checkout.paystack.com/usd-1",
                "reference": "pv-sub-usd-1"
            }
        })))
        .mount(&server)
        .await;

    let details = provider
        .create_subscription_checkout(&sub_request(
            50_000,
            Currency::Usd,
            BillingInterval::Annual,
            "alice@example.com",
            "pv-sub-usd-1",
        ))
        .await
        .unwrap();
    assert_eq!(details.provider_checkout_id, "pv-sub-usd-1");
}

// ============================================================================
// verify_payment_minor_units
// ============================================================================

#[tokio::test]
async fn verify_payment_minor_units_accepts_ngn_without_wallet_conversion() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-sub-ngn-1"))
        .and(header("authorization", AUTH_HEADER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "data": {
                "id": 9876543,
                "status": "success",
                "reference": "pv-sub-ngn-1",
                "amount": 800_000,      // ₦8,000 in kobo (raw)
                "currency": "NGN"
            }
        })))
        .mount(&server)
        .await;

    match provider
        .verify_payment_minor_units("pv-sub-ngn-1")
        .await
        .unwrap()
    {
        SubscriptionPaymentVerification::Paid {
            amount_minor,
            currency,
            provider_payment_id,
            provider_reference,
        } => {
            assert_eq!(amount_minor, 800_000); // NOT ×10_000 micro-kobo
            assert_eq!(currency, Currency::Ngn);
            assert_eq!(provider_payment_id, "9876543");
            assert_eq!(provider_reference, "pv-sub-ngn-1");
        }
        other => panic!("expected Paid, got {other:?}"),
    }
}

#[tokio::test]
async fn verify_payment_minor_units_accepts_usd_without_wallet_conversion() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-sub-usd-1"))
        .and(header("authorization", AUTH_HEADER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "data": {
                "id": 42,
                "status": "success",
                "reference": "pv-sub-usd-1",
                "amount": 5_000,       // $50.00 in cents
                "currency": "USD"
            }
        })))
        .mount(&server)
        .await;

    match provider
        .verify_payment_minor_units("pv-sub-usd-1")
        .await
        .unwrap()
    {
        SubscriptionPaymentVerification::Paid {
            amount_minor,
            currency,
            ..
        } => {
            assert_eq!(amount_minor, 5_000); // NOT ×10_000
            assert_eq!(currency, Currency::Usd);
        }
        other => panic!("expected Paid, got {other:?}"),
    }
}

#[tokio::test]
async fn verify_payment_minor_units_maps_pending_and_failed() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-pending"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "data": {
                "id": 1, "status": "pending", "reference": "pv-pending",
                "amount": 800_000, "currency": "NGN"
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-failed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "data": {
                "id": 2, "status": "failed", "reference": "pv-failed",
                "amount": 800_000, "currency": "NGN"
            }
        })))
        .mount(&server)
        .await;

    assert!(matches!(
        provider.verify_payment_minor_units("pv-pending").await.unwrap(),
        SubscriptionPaymentVerification::Pending { .. }
    ));
    assert!(matches!(
        provider.verify_payment_minor_units("pv-failed").await.unwrap(),
        SubscriptionPaymentVerification::Failed { .. }
    ));
}

#[tokio::test]
async fn verify_payment_minor_units_rejects_unsupported_currency() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-eur"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "data": {
                "id": 3, "status": "success", "reference": "pv-eur",
                "amount": 1000, "currency": "EUR"
            }
        })))
        .mount(&server)
        .await;

    let err = provider
        .verify_payment_minor_units("pv-eur")
        .await
        .unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("currency"),
        "expected unknown-currency error, got: {err}"
    );
}
