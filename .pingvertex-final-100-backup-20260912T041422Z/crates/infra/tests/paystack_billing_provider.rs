//! Contract tests for `PaystackBillingProvider`.
//!
//! These tests stand up a local `wiremock` HTTP server and point the
//! provider at it via `with_base_url`. They verify:
//!
//!   * Constructor validation (empty / whitespace secret key rejection).
//!   * Provider identity (`provider_name() == "paystack"`).
//!   * The exact HTTP shape of every outbound call:
//!     method, path, `Authorization: Bearer <secret>` header, and JSON body
//!     field names and values, including amount and currency conversions.
//!   * The Paystack response envelope interpretation: `status: bool`,
//!     optional `message`, optional `data: T | null`, and HTTP status.
//!   * The Paystack transaction status → `PaymentVerification` mapping for
//!     `success`, `pending`, and `failed`.
//!   * Error paths that must fail before any network call is made
//!     (missing email, non-USD currency, non-whole-cent amount, missing
//!     saved authorization).
//!
//! No live network is used. No `DATABASE_URL` is required.

use application::ports::{
    CheckoutRequest, PaymentProvider, PaymentVerification, RefundRequest, WalletTopUpRequest,
};
use domain::enums::PlanTier;
use domain::shared::ids::TenantId;
use infra::PaystackBillingProvider;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "sk_test_paystack_contract";
const AUTH_HEADER: &str = "Bearer sk_test_paystack_contract";

fn tenant() -> TenantId {
    TenantId(uuid::Uuid::new_v4())
}

async fn provider_for(server: &MockServer) -> PaystackBillingProvider {
    PaystackBillingProvider::with_base_url(SECRET.to_owned(), server.uri())
        .expect("provider must construct with a non-empty secret and a valid base URL")
}

// ============================================================================
// Constructor / identity
// ============================================================================

#[test]
fn new_rejects_empty_secret_key() {
    let err = PaystackBillingProvider::new(String::new()).unwrap_err();
    assert!(err.to_string().contains("secret key is required"));
}

#[test]
fn new_rejects_whitespace_secret_key() {
    let err = PaystackBillingProvider::new("   ".to_owned()).unwrap_err();
    assert!(err.to_string().contains("secret key is required"));
}

#[test]
fn new_accepts_a_non_empty_secret_key() {
    assert!(PaystackBillingProvider::new(SECRET.to_owned()).is_ok());
}

#[test]
fn provider_name_is_paystack() {
    let provider = PaystackBillingProvider::new(SECRET.to_owned()).unwrap();
    assert_eq!(provider.provider_name(), "paystack");
}

// ============================================================================
// create_checkout → POST /transaction/initialize
// ============================================================================

#[tokio::test]
async fn create_checkout_requires_email_before_any_network_call() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    let err = provider
        .create_checkout(&CheckoutRequest {
            tenant_id: tenant(),
            plan_tier: PlanTier::Free,
            amount_minor: 500,
            currency: "USD".to_owned(),
            customer_email: None,
        })
        .await
        .unwrap_err();

    assert!(err.to_string().contains("customer email is required"));
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "no HTTP call may be made when the email is missing"
    );
}

#[tokio::test]
async fn create_checkout_rejects_blank_email() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    let err = provider
        .create_checkout(&CheckoutRequest {
            tenant_id: tenant(),
            plan_tier: PlanTier::Free,
            amount_minor: 500,
            currency: "USD".to_owned(),
            customer_email: Some("   ".to_owned()),
        })
        .await
        .unwrap_err();

    assert!(err.to_string().contains("customer email is required"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn create_checkout_posts_expected_request_and_reads_response() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/initialize"))
        .and(header("authorization", AUTH_HEADER))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "Authorization URL created",
            "data": {
                "authorization_url": "https://checkout.paystack.com/abc123",
                "reference": "pv-sub-xyz"
            }
        })))
        .mount(&server)
        .await;

    let details = provider
        .create_checkout(&CheckoutRequest {
            tenant_id: tenant(),
            plan_tier: PlanTier::Free,
            amount_minor: 500,
            currency: "USD".to_owned(),
            customer_email: Some("alice@example.com".to_owned()),
        })
        .await
        .unwrap();

    assert_eq!(details.checkout_url, "https://checkout.paystack.com/abc123");
    assert_eq!(details.provider_checkout_id, "pv-sub-xyz");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["email"], "alice@example.com");
    // Subscription checkout amount is passed through unmodified.
    assert_eq!(body["amount"], 500);
    assert_eq!(body["currency"], "USD");
    assert!(
        body["reference"].as_str().unwrap().starts_with("pv-sub-"),
        "reference {:?} must start with pv-sub-",
        body["reference"]
    );
}

#[tokio::test]
async fn create_checkout_propagates_status_false_as_error() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/initialize"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": false,
            "message": "Invalid amount",
            "data": null
        })))
        .mount(&server)
        .await;

    let err = provider
        .create_checkout(&CheckoutRequest {
            tenant_id: tenant(),
            plan_tier: PlanTier::Free,
            amount_minor: 500,
            currency: "USD".to_owned(),
            customer_email: Some("alice@example.com".to_owned()),
        })
        .await
        .unwrap_err();

    assert!(err.to_string().contains("Invalid amount"));
}

#[tokio::test]
async fn create_checkout_propagates_http_5xx_message() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/initialize"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "status": false,
            "message": "Internal server error",
            "data": null
        })))
        .mount(&server)
        .await;

    let err = provider
        .create_checkout(&CheckoutRequest {
            tenant_id: tenant(),
            plan_tier: PlanTier::Free,
            amount_minor: 500,
            currency: "USD".to_owned(),
            customer_email: Some("alice@example.com".to_owned()),
        })
        .await
        .unwrap_err();

    assert!(err.to_string().contains("Internal server error"));
}

#[tokio::test]
async fn create_checkout_rejects_missing_data_payload() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/initialize"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "OK",
            "data": null
        })))
        .mount(&server)
        .await;

    let err = provider
        .create_checkout(&CheckoutRequest {
            tenant_id: tenant(),
            plan_tier: PlanTier::Free,
            amount_minor: 500,
            currency: "USD".to_owned(),
            customer_email: Some("alice@example.com".to_owned()),
        })
        .await
        .unwrap_err();

    assert!(err.to_string().contains("no checkout data"));
}

// ============================================================================
// verify_payment → GET /transaction/verify/{ref}
// ============================================================================

#[tokio::test]
async fn verify_payment_success_maps_to_paid_and_converts_minor_to_units() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-wallet-abc"))
        .and(header("authorization", AUTH_HEADER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "Verification successful",
            "data": {
                "id": 302961,
                "status": "success",
                "reference": "pv-wallet-abc",
                "amount": 500,
                "currency": "usd"
            }
        })))
        .mount(&server)
        .await;

    let verification = provider.verify_payment("pv-wallet-abc").await.unwrap();

    match verification {
        PaymentVerification::Paid {
            provider_payment_id,
            provider_reference,
            amount_units,
            currency,
        } => {
            assert_eq!(provider_payment_id, "302961");
            assert_eq!(provider_reference, "pv-wallet-abc");
            // 500 minor USD cents * 10_000 = 5_000_000 micro-USD.
            assert_eq!(amount_units, 5_000_000);
            // Provider lowercases the currency; the provider normalizes to
            // upper case before returning it.
            assert_eq!(currency, "USD");
        }
        other => panic!("expected Paid, got {other:?}"),
    }
}

#[tokio::test]
async fn verify_payment_pending_maps_to_pending() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-wallet-pending"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "Verification successful",
            "data": {
                "id": 1,
                "status": "pending",
                "reference": "pv-wallet-pending",
                "amount": 1000,
                "currency": "USD"
            }
        })))
        .mount(&server)
        .await;

    let verification = provider.verify_payment("pv-wallet-pending").await.unwrap();
    assert!(matches!(verification, PaymentVerification::Pending { .. }));
}

#[tokio::test]
async fn verify_payment_failed_maps_to_failed() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-wallet-fail"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "Verification successful",
            "data": {
                "id": 2,
                "status": "failed",
                "reference": "pv-wallet-fail",
                "amount": 1000,
                "currency": "USD"
            }
        })))
        .mount(&server)
        .await;

    let verification = provider.verify_payment("pv-wallet-fail").await.unwrap();
    assert!(matches!(verification, PaymentVerification::Failed { .. }));
}

#[tokio::test]
async fn verify_payment_rejects_non_usd_currency() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-ngn"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "Verification successful",
            "data": {
                "id": 3,
                "status": "success",
                "reference": "pv-ngn",
                "amount": 1000,
                "currency": "NGN"
            }
        })))
        .mount(&server)
        .await;

    let err = provider.verify_payment("pv-ngn").await.unwrap_err();
    assert!(
        err.to_string().contains("USD only"),
        "expected USD-only rejection, got: {err}"
    );
}

// ============================================================================
// create_wallet_top_up → POST /transaction/initialize (with conversion)
// ============================================================================

#[tokio::test]
async fn create_wallet_top_up_rejects_non_usd_before_any_network_call() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    let err = provider
        .create_wallet_top_up(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "NGN".to_owned(),
            idempotency_key: "key".to_owned(),
            payment_method: None,
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(err.to_string().contains("currency must be USD"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn create_wallet_top_up_rejects_blank_email_before_any_network_call() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    let err = provider
        .create_wallet_top_up(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key".to_owned(),
            payment_method: None,
            customer_email: "   ".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(err.to_string().contains("customer email is required"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn create_wallet_top_up_converts_units_to_minor_units() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/initialize"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "Authorization URL created",
            "data": {
                "authorization_url": "https://checkout.paystack.com/abc",
                "reference": "pv-wallet-xyz"
            }
        })))
        .mount(&server)
        .await;

    // $5 = 5_000_000 micro-USD = 500 USD cents (minor units).
    let details = provider
        .create_wallet_top_up(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key".to_owned(),
            payment_method: None,
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap();
    assert_eq!(details.provider_checkout_id, "pv-wallet-xyz");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["email"], "alice@example.com");
    assert_eq!(
        body["amount"], 500,
        "wallet units must be converted to cents"
    );
    assert_eq!(body["currency"], "USD");
    assert!(
        body["reference"]
            .as_str()
            .unwrap()
            .starts_with("pv-wallet-"),
        "reference {:?} must start with pv-wallet-",
        body["reference"]
    );
}

#[tokio::test]
async fn create_wallet_top_up_rejects_non_whole_cent_amounts() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    // 5_000_500 micro-USD = $5.0005, which is NOT a whole number of cents.
    let err = provider
        .create_wallet_top_up(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_500,
            currency: "USD".to_owned(),
            idempotency_key: "key".to_owned(),
            payment_method: None,
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("whole USD cent"),
        "expected whole-cent rejection, got: {err}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ============================================================================
// charge_saved_payment_method → POST /transaction/charge_authorization
// ============================================================================

#[tokio::test]
async fn charge_saved_payment_method_requires_authorization_before_any_network_call() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    let err = provider
        .charge_saved_payment_method(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key-1".to_owned(),
            payment_method: None,
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("saved payment authorization is required"),
        "got: {err}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn charge_saved_payment_method_posts_expected_request() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/charge_authorization"))
        .and(header("authorization", AUTH_HEADER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "Charge attempted",
            "data": {
                "id": 98765,
                "status": "success",
                "reference": "pv-auto-key-1",
                "amount": 500,
                "currency": "USD"
            }
        })))
        .mount(&server)
        .await;

    let verification = provider
        .charge_saved_payment_method(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key-1".to_owned(),
            payment_method: Some("AUTH_abc123".to_owned()),
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap();

    match verification {
        PaymentVerification::Paid {
            provider_payment_id,
            provider_reference,
            amount_units,
            ..
        } => {
            assert_eq!(provider_payment_id, "98765");
            assert_eq!(provider_reference, "pv-auto-key-1");
            assert_eq!(amount_units, 5_000_000);
        }
        other => panic!("expected Paid, got {other:?}"),
    }

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["email"], "alice@example.com");
    assert_eq!(body["amount"], 500);
    assert_eq!(body["authorization_code"], "AUTH_abc123");
    // The reference is derived from the idempotency key with the `pv-auto-`
    // prefix, which is what the auto-top-up worker relies on for retries.
    assert_eq!(body["reference"], "pv-auto-key-1");
    assert_eq!(body["currency"], "USD");
}

#[tokio::test]
async fn charge_saved_payment_method_maps_pending_status() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/charge_authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "Charge attempted",
            "data": {
                "id": 98766,
                "status": "queued",
                "reference": "pv-auto-key-2",
                "amount": 500,
                "currency": "USD"
            }
        })))
        .mount(&server)
        .await;

    let verification = provider
        .charge_saved_payment_method(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key-2".to_owned(),
            payment_method: Some("AUTH_abc123".to_owned()),
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap();

    assert!(matches!(verification, PaymentVerification::Pending { .. }));
}

// ============================================================================
// refund_payment → POST /refund
// ============================================================================

#[tokio::test]
async fn refund_payment_posts_expected_request_and_reads_refund_id() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/refund"))
        .and(header("authorization", AUTH_HEADER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": true,
            "message": "Refund has been queued for processing",
            "data": {
                "id": 55555
            }
        })))
        .mount(&server)
        .await;

    let details = provider
        .refund_payment(&RefundRequest {
            tenant_id: tenant(),
            provider_payment_id: "302961".to_owned(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "refund-1".to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(details.provider_refund_id, "55555");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["transaction"], "302961");
    assert_eq!(body["amount"], 500);
    assert_eq!(body["currency"], "USD");
}

#[tokio::test]
async fn refund_payment_rejects_non_whole_cent_amounts_before_any_network_call() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    let err = provider
        .refund_payment(&RefundRequest {
            tenant_id: tenant(),
            provider_payment_id: "302961".to_owned(),
            amount_units: 5_000_500,
            currency: "USD".to_owned(),
            idempotency_key: "refund-1".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(err.to_string().contains("whole USD cent"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn refund_payment_propagates_status_false_as_error() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/refund"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": false,
            "message": "Refund window has closed",
            "data": null
        })))
        .mount(&server)
        .await;

    let err = provider
        .refund_payment(&RefundRequest {
            tenant_id: tenant(),
            provider_payment_id: "302961".to_owned(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "refund-1".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(err.to_string().contains("Refund window has closed"));
}

// ============================================================================
// Task 5 completion matrix
// ----------------------------------------------------------------------------
// Adds coverage for the matrix rows the original contract file omitted:
// malformed JSON responses, unknown transactions, provider rejections on
// the charge path, immediate network failures, and secret-redaction
// guarantees. Timeout tests are marked `#[ignore]` because the provider's
// reqwest client uses a fixed 15-second timeout and gating them by default
// would add ~30s to every `cargo test` run.
// ============================================================================

/// Helper: bind and immediately release a local TCP port so we have a
/// `127.0.0.1:<port>` that is guaranteed to refuse connections. Used to
/// test network-error paths deterministically (no wiremock shutdown race).
fn unreachable_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}")
}

async fn provider_at(base_url: String) -> PaystackBillingProvider {
    PaystackBillingProvider::with_base_url(SECRET.to_owned(), base_url)
        .expect("provider must construct with a non-empty secret and a valid base URL")
}

// ----------------------------------------------------------------------------
// CHECKOUT: malformed JSON, network error, timeout
// ----------------------------------------------------------------------------

#[tokio::test]
async fn create_checkout_rejects_malformed_json_response() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/initialize"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>not json</html>"))
        .mount(&server)
        .await;

    let err = provider
        .create_checkout(&CheckoutRequest {
            tenant_id: tenant(),
            plan_tier: PlanTier::Free,
            amount_minor: 500,
            currency: "USD".to_owned(),
            customer_email: Some("alice@example.com".to_owned()),
        })
        .await
        .unwrap_err();

    // The exact wording of a serde decode error is not part of the
    // contract; the important guarantee is that we surface it as an Err
    // rather than pretending the checkout succeeded.
    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn create_checkout_propagates_network_error() {
    let provider = provider_at(unreachable_base_url()).await;

    let err = provider
        .create_checkout(&CheckoutRequest {
            tenant_id: tenant(),
            plan_tier: PlanTier::Free,
            amount_minor: 500,
            currency: "USD".to_owned(),
            customer_email: Some("alice@example.com".to_owned()),
        })
        .await
        .unwrap_err();

    assert!(!err.to_string().is_empty());
}

#[tokio::test]
#[ignore = "runs in ~15s because reqwest uses a fixed 15s timeout; run with --ignored"]
async fn create_checkout_timeout_is_bounded() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/initialize"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_secs(30))
                .set_body_json(json!({"status": true, "data": null})),
        )
        .mount(&server)
        .await;

    let err = provider
        .create_checkout(&CheckoutRequest {
            tenant_id: tenant(),
            plan_tier: PlanTier::Free,
            amount_minor: 500,
            currency: "USD".to_owned(),
            customer_email: Some("alice@example.com".to_owned()),
        })
        .await
        .unwrap_err();

    assert!(!err.to_string().is_empty());
}

// ----------------------------------------------------------------------------
// VERIFY: unknown transaction, malformed JSON, network error
// ----------------------------------------------------------------------------

#[tokio::test]
async fn verify_payment_unknown_transaction_returns_error() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-unknown"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "status": false,
            "message": "Transaction not found",
            "data": null
        })))
        .mount(&server)
        .await;

    let err = provider.verify_payment("pv-unknown").await.unwrap_err();
    assert!(
        err.to_string().contains("Transaction not found"),
        "expected provider message, got: {err}"
    );
}

#[tokio::test]
async fn verify_payment_rejects_malformed_json_response() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-malformed"))
        .respond_with(ResponseTemplate::new(200).set_body_string("}{ not json"))
        .mount(&server)
        .await;

    let err = provider.verify_payment("pv-malformed").await.unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn verify_payment_propagates_network_error() {
    let provider = provider_at(unreachable_base_url()).await;
    let err = provider.verify_payment("pv-wallet-abc").await.unwrap_err();
    assert!(!err.to_string().is_empty());
}

// ----------------------------------------------------------------------------
// CHARGE SAVED AUTHORIZATION: provider rejection, malformed, network error
// ----------------------------------------------------------------------------

#[tokio::test]
async fn charge_saved_payment_method_propagates_provider_rejection() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/charge_authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": false,
            "message": "Charge attempted but rejected by issuer",
            "data": null
        })))
        .mount(&server)
        .await;

    let err = provider
        .charge_saved_payment_method(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key-rej".to_owned(),
            payment_method: Some("AUTH_xyz".to_owned()),
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("rejected by issuer"),
        "expected provider rejection message, got: {err}"
    );
}

#[tokio::test]
async fn charge_saved_payment_method_rejects_malformed_response() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/charge_authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;

    let err = provider
        .charge_saved_payment_method(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key-bad".to_owned(),
            payment_method: Some("AUTH_xyz".to_owned()),
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn charge_saved_payment_method_propagates_network_error() {
    let provider = provider_at(unreachable_base_url()).await;

    let err = provider
        .charge_saved_payment_method(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key-net".to_owned(),
            payment_method: Some("AUTH_xyz".to_owned()),
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(!err.to_string().is_empty());
}

#[tokio::test]
#[ignore = "runs in ~15s because reqwest uses a fixed 15s timeout; run with --ignored"]
async fn charge_saved_payment_method_timeout_is_bounded() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/charge_authorization"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
        .mount(&server)
        .await;

    let err = provider
        .charge_saved_payment_method(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key-slow".to_owned(),
            payment_method: Some("AUTH_xyz".to_owned()),
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(!err.to_string().is_empty());
}

// ----------------------------------------------------------------------------
// REFUND: malformed response, network error, timeout
// ----------------------------------------------------------------------------

#[tokio::test]
async fn refund_payment_rejects_malformed_response() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/refund"))
        .respond_with(ResponseTemplate::new(200).set_body_string("oops"))
        .mount(&server)
        .await;

    let err = provider
        .refund_payment(&RefundRequest {
            tenant_id: tenant(),
            provider_payment_id: "302961".to_owned(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "refund-bad".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn refund_payment_propagates_network_error() {
    let provider = provider_at(unreachable_base_url()).await;

    let err = provider
        .refund_payment(&RefundRequest {
            tenant_id: tenant(),
            provider_payment_id: "302961".to_owned(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "refund-net".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(!err.to_string().is_empty());
}

#[tokio::test]
#[ignore = "runs in ~15s because reqwest uses a fixed 15s timeout; run with --ignored"]
async fn refund_payment_timeout_is_bounded() {
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/refund"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
        .mount(&server)
        .await;

    let err = provider
        .refund_payment(&RefundRequest {
            tenant_id: tenant(),
            provider_payment_id: "302961".to_owned(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "refund-slow".to_owned(),
        })
        .await
        .unwrap_err();

    assert!(!err.to_string().is_empty());
}

// ----------------------------------------------------------------------------
// SECURITY: no secret / credential leakage
// ----------------------------------------------------------------------------

#[test]
fn provider_debug_does_not_leak_secret_key() {
    const UNIQUE_SECRET: &str = "sk_test_unique_should_never_leak_9a8b7c6d";
    let provider = PaystackBillingProvider::new(UNIQUE_SECRET.to_owned()).unwrap();
    let debug = format!("{provider:?}");
    assert!(
        !debug.contains(UNIQUE_SECRET),
        "Debug output must not contain the secret key; got: {debug}"
    );
    assert!(
        debug.contains("PaystackBillingProvider"),
        "Debug output must identify the type: {debug}"
    );
}

#[tokio::test]
async fn provider_error_messages_do_not_leak_secret_key() {
    const UNIQUE_SECRET: &str = "sk_test_unique_should_never_leak_5e4d3c2b";
    let server = MockServer::start().await;
    let provider =
        PaystackBillingProvider::with_base_url(UNIQUE_SECRET.to_owned(), server.uri()).unwrap();

    Mock::given(method("GET"))
        .and(path("/transaction/verify/pv-leak"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "status": false,
            "message": "Transaction not found",
            "data": null
        })))
        .mount(&server)
        .await;

    let err = provider.verify_payment("pv-leak").await.unwrap_err();
    let msg = err.to_string();
    assert!(
        !msg.contains(UNIQUE_SECRET),
        "provider error must not contain the secret key; got: {msg}"
    );

    // Also verify the outbound request header did carry the secret (so
    // the test really exercises the credential path) — this sanity-check
    // ensures the redaction assertion above is meaningful.
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let auth = requests[0]
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(auth.starts_with("Bearer "));
}

#[tokio::test]
async fn charge_saved_error_does_not_leak_authorization_code() {
    const UNIQUE_AUTH: &str = "AUTH_sensitive_card_token_1234567890";
    let server = MockServer::start().await;
    let provider = provider_for(&server).await;

    Mock::given(method("POST"))
        .and(path("/transaction/charge_authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": false,
            "message": "Charge rejected",
            "data": null
        })))
        .mount(&server)
        .await;

    let err = provider
        .charge_saved_payment_method(&WalletTopUpRequest {
            tenant_id: tenant(),
            amount_units: 5_000_000,
            currency: "USD".to_owned(),
            idempotency_key: "key-sec".to_owned(),
            payment_method: Some(UNIQUE_AUTH.to_owned()),
            customer_email: "alice@example.com".to_owned(),
        })
        .await
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        !msg.contains(UNIQUE_AUTH),
        "provider error must not contain the saved authorization code; got: {msg}"
    );
}
