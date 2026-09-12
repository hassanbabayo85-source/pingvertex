use application::{
    commands::ProcessBillingWebhookCommand, use_cases::ProcessBillingWebhookUseCase,
};
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use domain::enums::BillingWebhookEventType;
use domain::shared::ids::TenantId;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha512;

use crate::error::ApiError;
use crate::state::AppState;

type HmacSha512 = Hmac<Sha512>;

/// Must match `PaystackBillingProvider::provider_name()`.
const PAYSTACK_PROVIDER_NAME: &str = "paystack";

/// Hard cap on the raw request body. Paystack webhook bodies are a few
/// KB; 256 KiB is generous and prevents memory exhaustion.
const MAX_BODY_BYTES: usize = 256 * 1024;

/// Paystack wallet top-up amounts are USD minor units (cents). PingVertex
/// wallet units are micro-USD, so one cent is 10_000 units.
const MINOR_UNITS_PER_WALLET_UNIT: i64 = 10_000;

#[derive(Debug, Deserialize)]
struct PaystackEnvelope {
    event: String,
    data: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct PaystackWebhookResponse {
    pub status: String,
}

/// Paystack webhook endpoint.
///
/// Kept separate from `/billing/webhook` (the internal shared-secret
/// endpoint) because Paystack's payload shape, signature algorithm, and
/// header name all differ from the internal contract.
///
/// Supported events:
///   * `charge.success`   — credit the wallet top-up session's owner.
///   * `charge.failed`    — mark the top-up session FAILED (no credit).
///   * `refund.processed` — credit a verified refund against the original
///     completed payment.
///
/// All other Paystack events (transfer.*, dispute.*, subscription.*, ...)
/// return 200 with `{"status":"ignored"}` so Paystack does not retry them.
pub async fn paystack_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<PaystackWebhookResponse>), ApiError> {
    let secret = state
        .billing_webhook_secret
        .as_ref()
        .ok_or_else(|| ApiError::internal("billing webhook secret is not configured"))?;

    if body.len() > MAX_BODY_BYTES {
        return Err(ApiError::bad_request(
            "paystack webhook payload is too large",
        ));
    }

    verify_paystack_signature(secret, &headers, &body)?;

    let envelope: PaystackEnvelope = serde_json::from_slice(&body)
        .map_err(|_| ApiError::bad_request("invalid paystack webhook payload"))?;

    match envelope.event.as_str() {
        "charge.success" => handle_charge_success(&state, envelope).await,
        "charge.failed" => handle_charge_failed(&state, envelope).await,
        "refund.processed" => handle_refund_processed(&state, envelope).await,
        other => {
            tracing::info!(
                event = %other,
                "paystack webhook received for unsupported event type; ignoring"
            );
            Ok((
                StatusCode::OK,
                Json(PaystackWebhookResponse {
                    status: "ignored".to_owned(),
                }),
            ))
        }
    }
}

/// `charge.success` — credit a wallet top-up.
///
/// Paystack does not include a tenant id; the wallet top-up session is
/// looked up by `(provider, provider_checkout_id)` where
/// `provider_checkout_id` is the Paystack reference we generated at
/// `create_wallet_top_up` time. The unique index added in migration 0081
/// (`uq_wallet_topup_provider_checkout`) makes this lookup unambiguous
/// across tenants.
async fn handle_charge_success(
    state: &AppState,
    envelope: PaystackEnvelope,
) -> Result<(StatusCode, Json<PaystackWebhookResponse>), ApiError> {
    let reference = envelope
        .data
        .get("reference")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad_request("paystack webhook missing reference"))?;

    let provider_payment_id = envelope
        .data
        .get("id")
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
        })
        .map(|n| n.to_string())
        .ok_or_else(|| ApiError::bad_request("paystack webhook missing transaction id"))?;

    let provider_amount_minor = envelope
        .data
        .get("amount")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError::bad_request("paystack webhook missing amount"))?;

    let currency = envelope
        .data
        .get("currency")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("paystack webhook missing currency"))?;

    if !currency.eq_ignore_ascii_case("USD") {
        return Err(ApiError::bad_request(
            "paystack webhook currency must be USD",
        ));
    }

    let amount_units = provider_amount_minor
        .checked_mul(MINOR_UNITS_PER_WALLET_UNIT)
        .ok_or_else(|| ApiError::bad_request("paystack webhook amount overflow"))?;

    if amount_units <= 0 {
        return Err(ApiError::bad_request(
            "paystack webhook amount must be positive",
        ));
    }

    let session = state
        .wallet_repository
        .find_top_up_session_global(PAYSTACK_PROVIDER_NAME, reference)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load wallet top-up session", e))?;

    let Some(session) = session else {
        tracing::warn!(
            reference,
            "paystack charge.success for unknown wallet top-up session; ignoring \
             (reconciliation worker will pick up any missing credit)"
        );
        return Ok((
            StatusCode::OK,
            Json(PaystackWebhookResponse {
                status: "unknown_reference".to_owned(),
            }),
        ));
    };

    // Paystack does not include a unique webhook id. Derive a stable
    // provider_event_id from (event, transaction id) so a Paystack retry
    // of the same charge is deduplicated by the idempotency store.
    let provider_event_id = format!("paystack:charge.success:{provider_payment_id}");

    let payload = serde_json::json!({
        "wallet_top_up": true,
        "provider_checkout_id": session.provider_checkout_id,
        "payment_id": provider_payment_id,
        "amount_units": amount_units,
        "currency": currency.to_ascii_uppercase(),
    });

    let command = ProcessBillingWebhookCommand {
        provider_event_id,
        event_type: BillingWebhookEventType::PaymentSucceeded,
        tenant_id: TenantId(session.tenant_id),
        plan_tier: None,
        cancel_at_period_end: false,
        current_period_ends_at: None,
        payload,
    };

    let use_case = ProcessBillingWebhookUseCase::new(
        state.billing_webhook_idempotency_repository.clone(),
        state.billing_audit_log_repository.clone(),
        state.subscription_repository.clone(),
    )
    .with_wallet_repository(state.wallet_repository.clone())
    .with_monitor_repository(state.monitor_repository.clone())
    .with_payment_repository(state.payment_repository.clone())
    .with_financial_audit_repository(state.financial_audit_log_repository.clone());

    let outcome = use_case
        .execute(command)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to process paystack webhook", err))?;

    Ok((
        StatusCode::OK,
        Json(PaystackWebhookResponse {
            status: if outcome.processed {
                "processed"
            } else {
                "duplicate"
            }
            .to_owned(),
        }),
    ))
}

/// `charge.failed` — mark the wallet top-up session FAILED.
///
/// No wallet credit. No use case invocation: `ProcessBillingWebhookUseCase`
/// only acts on `wallet_top_up=true` when the event type is PaymentSucceeded
/// and would otherwise fall through to the subscription path (which would
/// fail with SubscriptionNotFound for a wallet-only tenant). The state
/// transition is a direct, idempotent repository update instead.
async fn handle_charge_failed(
    state: &AppState,
    envelope: PaystackEnvelope,
) -> Result<(StatusCode, Json<PaystackWebhookResponse>), ApiError> {
    let reference = envelope
        .data
        .get("reference")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::bad_request("paystack charge.failed missing reference"))?;

    let session = state
        .wallet_repository
        .find_top_up_session_global(PAYSTACK_PROVIDER_NAME, reference)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load wallet top-up session", e))?;

    let Some(session) = session else {
        tracing::warn!(
            reference,
            "paystack charge.failed for unknown wallet top-up session; ignoring"
        );
        return Ok((
            StatusCode::OK,
            Json(PaystackWebhookResponse {
                status: "unknown_reference".to_owned(),
            }),
        ));
    };

    let marked = state
        .wallet_repository
        .mark_top_up_session_failed(
            session.tenant_id,
            &session.provider_checkout_id,
            "paystack charge.failed",
        )
        .await
        .map_err(|e| {
            ApiError::internal_with_source("failed to mark wallet top-up session failed", e)
        })?;

    Ok((
        StatusCode::OK,
        Json(PaystackWebhookResponse {
            status: if marked {
                "processed"
            } else {
                "already_settled"
            }
            .to_owned(),
        }),
    ))
}

/// `refund.processed` — credit a verified refund against the original
/// completed wallet top-up payment.
///
/// Paystack's refund webhook payload shape (documented at
/// https://paystack.com/docs/payments/refunds/#refund-webhook):
///
/// ```json
/// {
///   "event": "refund.processed",
///   "data": {
///     "id": <refund_id>,
///     "amount": <refund_amount_minor>,
///     "currency": "USD",
///     "status": "processed",
///     "transaction": {
///     "id": <transaction_id>,
///     "reference": "<original reference>",
///     "amount": <original_amount_minor>,
///     "currency": "USD",
///     "status": "success"
///     }
///   }
/// }
/// ```
///
/// The session is looked up by `data.transaction.reference` (which is the
/// same string we stored as `provider_checkout_id` at top-up creation
/// time). The `payment_id` passed to the wallet's `record_verified_refund`
/// is the session's own `provider_payment_id`, which was recorded during
/// `charge.success` — this keeps the refund keyed to the same identity the
/// wallet ledger used for the original credit.
async fn handle_refund_processed(
    state: &AppState,
    envelope: PaystackEnvelope,
) -> Result<(StatusCode, Json<PaystackWebhookResponse>), ApiError> {
    let tx = envelope
        .data
        .get("transaction")
        .ok_or_else(|| ApiError::bad_request("paystack refund webhook missing transaction"))?;

    let reference = tx
        .get("reference")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError::bad_request("paystack refund webhook missing transaction reference")
        })?;

    let refund_id = envelope
        .data
        .get("id")
        .and_then(|v| {
            v.as_i64()
                .map(|n| n.to_string())
                .or_else(|| v.as_str().map(|s| s.to_owned()))
        })
        .ok_or_else(|| ApiError::bad_request("paystack refund webhook missing refund id"))?;

    let refund_amount_minor = envelope
        .data
        .get("amount")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError::bad_request("paystack refund webhook missing amount"))?;

    let currency = envelope
        .data
        .get("currency")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("paystack refund webhook missing currency"))?;

    if !currency.eq_ignore_ascii_case("USD") {
        return Err(ApiError::bad_request(
            "paystack refund webhook currency must be USD",
        ));
    }

    let amount_units = refund_amount_minor
        .checked_mul(MINOR_UNITS_PER_WALLET_UNIT)
        .ok_or_else(|| ApiError::bad_request("paystack refund webhook amount overflow"))?;

    if amount_units <= 0 {
        return Err(ApiError::bad_request(
            "paystack refund webhook amount must be positive",
        ));
    }

    let session = state
        .wallet_repository
        .find_top_up_session_global(PAYSTACK_PROVIDER_NAME, reference)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load wallet top-up session", e))?;

    let Some(session) = session else {
        tracing::warn!(
            reference,
            "paystack refund.processed for unknown wallet top-up session; ignoring"
        );
        return Ok((
            StatusCode::OK,
            Json(PaystackWebhookResponse {
                status: "unknown_reference".to_owned(),
            }),
        ));
    };

    let payment_id = session.provider_payment_id.clone().ok_or_else(|| {
        ApiError::bad_request("wallet top-up session has no provider payment id; cannot refund")
    })?;

    let provider_event_id = format!("paystack:refund.processed:{refund_id}");

    let payload = serde_json::json!({
        "amount_units": amount_units,
        "payment_id": payment_id,
        "refund_id": refund_id,
    });

    let command = ProcessBillingWebhookCommand {
        provider_event_id,
        event_type: BillingWebhookEventType::PaymentRefunded,
        tenant_id: TenantId(session.tenant_id),
        plan_tier: None,
        cancel_at_period_end: false,
        current_period_ends_at: None,
        payload,
    };

    let use_case = ProcessBillingWebhookUseCase::new(
        state.billing_webhook_idempotency_repository.clone(),
        state.billing_audit_log_repository.clone(),
        state.subscription_repository.clone(),
    )
    .with_wallet_repository(state.wallet_repository.clone())
    .with_monitor_repository(state.monitor_repository.clone())
    .with_payment_repository(state.payment_repository.clone())
    .with_financial_audit_repository(state.financial_audit_log_repository.clone());

    let outcome = use_case
        .execute(command)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to process paystack refund", err))?;

    Ok((
        StatusCode::OK,
        Json(PaystackWebhookResponse {
            status: if outcome.processed {
                "processed"
            } else {
                "duplicate"
            }
            .to_owned(),
        }),
    ))
}

/// Verifies a Paystack webhook signature.
///
/// Paystack sends `X-Paystack-Signature: <lowercase hex>` where the value
/// is HMAC-SHA512 over the raw request body, keyed by the Paystack secret
/// key. There is no `sha256=`/`sha512=` prefix. Comparison is constant-time
/// (`Mac::verify_slice`, backed by `subtle`).
pub fn verify_paystack_signature(
    secret: &str,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), ApiError> {
    let hex_sig = headers
        .get("x-paystack-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing paystack webhook signature"))?;

    let expected = hex::decode(hex_sig)
        .map_err(|_| ApiError::unauthorized("invalid paystack webhook signature format"))?;

    let mut mac = HmacSha512::new_from_slice(secret.as_bytes())
        .map_err(|_| ApiError::internal("failed to initialize paystack webhook verifier"))?;
    mac.update(body);

    mac.verify_slice(&expected)
        .map_err(|_| ApiError::unauthorized("invalid paystack webhook signature"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign_paystack(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha512::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn valid_paystack_signature_is_accepted() {
        let body = br#"{"event":"charge.success"}"#;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-paystack-signature",
            sign_paystack("sk_test_secret", body).parse().unwrap(),
        );
        assert!(verify_paystack_signature("sk_test_secret", &headers, body).is_ok());
    }

    #[test]
    fn missing_paystack_signature_is_rejected() {
        let headers = HeaderMap::new();
        assert!(verify_paystack_signature("sk_test_secret", &headers, b"{}").is_err());
    }

    #[test]
    fn wrong_secret_paystack_signature_is_rejected() {
        let body = b"{}";
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-paystack-signature",
            sign_paystack("wrong_secret", body).parse().unwrap(),
        );
        assert!(verify_paystack_signature("sk_test_secret", &headers, body).is_err());
    }

    #[test]
    fn non_hex_paystack_signature_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("x-paystack-signature", "not-hex-at-all!!".parse().unwrap());
        assert!(verify_paystack_signature("sk_test_secret", &headers, b"{}").is_err());
    }

    #[test]
    fn internal_style_header_is_not_accepted_on_paystack_endpoint() {
        let mut headers = HeaderMap::new();
        headers.insert("x-billing-signature", "sha256=deadbeef".parse().unwrap());
        assert!(verify_paystack_signature("sk_test_secret", &headers, b"{}").is_err());
    }

    #[test]
    fn envelope_deserializes_charge_failed() {
        let raw = br#"{"event":"charge.failed","data":{"reference":"pv-wallet-abc","id":12345}}"#;
        let env: PaystackEnvelope = serde_json::from_slice(raw).unwrap();
        assert_eq!(env.event, "charge.failed");
        assert_eq!(env.data["reference"], "pv-wallet-abc");
    }

    #[test]
    fn envelope_deserializes_refund_processed() {
        let raw = br#"{
            "event":"refund.processed",
            "data":{
                "id":999,
                "amount":500,
                "currency":"USD",
                "status":"processed",
                "transaction":{"id":12345,"reference":"pv-wallet-abc","amount":500,"currency":"USD","status":"success"}
            }
        }"#;
        let env: PaystackEnvelope = serde_json::from_slice(raw).unwrap();
        assert_eq!(env.event, "refund.processed");
        assert_eq!(env.data["id"], 999);
        assert_eq!(env.data["amount"], 500);
        assert_eq!(env.data["transaction"]["reference"], "pv-wallet-abc");
    }
}
