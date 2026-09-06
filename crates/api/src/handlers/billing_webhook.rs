use application::{
    commands::ProcessBillingWebhookCommand, use_cases::ProcessBillingWebhookUseCase,
};
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use domain::enums::{BillingWebhookEventType, PlanTier};
use domain::shared::ids::TenantId;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::ApiError;
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
pub struct BillingWebhookPayload {
    pub event_id: String,
    pub event_type: String,
    pub tenant_id: uuid::Uuid,
    #[serde(default)]
    pub plan_tier: Option<String>,
    #[serde(default)]
    pub addon_subscription_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub cancel_at_period_end: bool,
    #[serde(default)]
    pub current_period_ends_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct BillingWebhookResponse {
    pub status: String,
}

pub async fn billing_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<BillingWebhookResponse>), ApiError> {
    let secret = state
        .billing_webhook_secret
        .as_ref()
        .ok_or_else(|| ApiError::internal("billing webhook secret is not configured"))?;

    verify_signature(secret, &headers, &body)?;

    let payload: BillingWebhookPayload = serde_json::from_slice(&body)
        .map_err(|_| ApiError::bad_request("invalid billing webhook payload"))?;

    let event_type: BillingWebhookEventType = payload
        .event_type
        .parse()
        .map_err(|_| ApiError::bad_request("unknown billing webhook event type"))?;

    let plan_tier = payload
        .plan_tier
        .map(|s| s.parse::<PlanTier>())
        .transpose()
        .map_err(|_| ApiError::bad_request("invalid plan tier in webhook payload"))?;

    let command = ProcessBillingWebhookCommand {
        provider_event_id: payload.event_id,
        event_type,
        tenant_id: TenantId(payload.tenant_id),
        plan_tier,
        cancel_at_period_end: payload.cancel_at_period_end,
        current_period_ends_at: payload.current_period_ends_at,
        payload: {
            let mut d = payload.data;
            if let Some(id) = payload.addon_subscription_id {
                d["addon_subscription_id"] = serde_json::Value::String(id.to_string())
            }
            d
        },
    };

    let use_case = ProcessBillingWebhookUseCase::new(
        state.billing_webhook_idempotency_repository.clone(),
        state.billing_audit_log_repository.clone(),
        state.subscription_repository.clone(),
    )
    .with_addon_repository(state.add_on_subscription_repository.clone());

    let outcome = use_case
        .execute(command)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to process billing webhook", err))?;

    let status = if outcome.processed {
        "processed"
    } else {
        "duplicate"
    };

    Ok((
        StatusCode::OK,
        Json(BillingWebhookResponse {
            status: status.to_owned(),
        }),
    ))
}

/// Verifies the provider signature on the raw request body.
///
/// The signature header must be `X-Billing-Signature: sha256=<hex>`
/// (or `X-Webhook-Signature` with the same format). The HMAC secret is
/// supplied separately so this function is easy to unit-test without
/// constructing a full `AppState`.
///
/// SEC (B37 backend security audit): this used to compute the expected
/// signature as a hex `String` and compare it against the caller-supplied
/// header with `!=`. `String`/`&str` inequality short-circuits on the
/// first differing byte, which leaks timing information proportional to
/// how many leading bytes of the signature an attacker has correctly
/// guessed — a classic timing side-channel that lets a remote attacker
/// forge a valid signature byte-by-byte without ever knowing
/// `BILLING_WEBHOOK_SECRET`, since `POST /api/v1/billing/webhook` is
/// otherwise unauthenticated by design (signature verification IS the
/// authentication). This is the exact bug class `api::metrics::constant_time_eq`
/// was already introduced to close for `METRICS_TOKEN`; this handler had
/// never been brought up to that same standard.
///
/// Fixed by decoding the expected hex signature to raw bytes and using
/// `hmac::Mac::verify_slice`, which performs a constant-time comparison
/// internally (backed by the `subtle` crate) instead of a variable-time
/// string comparison.
pub fn verify_signature(secret: &str, headers: &HeaderMap, body: &[u8]) -> Result<(), ApiError> {
    let signature_header = headers
        .get("x-billing-signature")
        .or_else(|| headers.get("x-webhook-signature"))
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing billing webhook signature"))?;

    let expected_hex = signature_header
        .strip_prefix("sha256=")
        .ok_or_else(|| ApiError::unauthorized("invalid billing webhook signature format"))?;

    let expected_bytes = hex::decode(expected_hex)
        .map_err(|_| ApiError::unauthorized("invalid billing webhook signature format"))?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| ApiError::internal("failed to initialize billing webhook verifier"))?;
    mac.update(body);

    // Constant-time comparison — see doc comment above.
    mac.verify_slice(&expected_bytes)
        .map_err(|_| ApiError::unauthorized("invalid billing webhook signature"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn valid_signature_is_accepted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-billing-signature",
            sign("secret", b"{}").parse().unwrap(),
        );

        assert!(verify_signature("secret", &headers, b"{}").is_ok());
    }

    #[test]
    fn invalid_signature_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("x-billing-signature", "sha256=deadbeef".parse().unwrap());

        assert!(verify_signature("secret", &headers, b"{}").is_err());
    }

    #[test]
    fn missing_signature_is_rejected() {
        let headers = HeaderMap::new();
        assert!(verify_signature("secret", &headers, b"{}").is_err());
    }

    #[test]
    fn malformed_signature_header_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("x-billing-signature", "not-a-valid-format".parse().unwrap());

        assert!(verify_signature("secret", &headers, b"{}").is_err());
    }

    /// SEC regression: a non-hex signature value must be rejected as a
    /// format error (via `hex::decode` failing), not accepted or panic —
    /// this is the new failure path introduced by decoding to bytes
    /// before comparison.
    #[test]
    fn non_hex_signature_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-billing-signature",
            "sha256=not-hex-at-all!!".parse().unwrap(),
        );

        assert!(verify_signature("secret", &headers, b"{}").is_err());
    }

    /// SEC regression: a same-length but byte-wise-different signature
    /// (the case a naive `!=` string compare would also catch, but where
    /// the constant-time path must still correctly reject it) is rejected.
    #[test]
    fn same_length_wrong_signature_is_rejected() {
        let correct = sign("secret", b"{}");
        let correct_hex = correct.strip_prefix("sha256=").unwrap();
        // Flip the first hex character while preserving length.
        let mut chars: Vec<char> = correct_hex.chars().collect();
        chars[0] = if chars[0] == '0' { '1' } else { '0' };
        let tampered: String = chars.into_iter().collect();

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-billing-signature",
            format!("sha256={tampered}").parse().unwrap(),
        );

        assert!(verify_signature("secret", &headers, b"{}").is_err());
    }
}
