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
        payload: payload.data,
    };

    let use_case = ProcessBillingWebhookUseCase::new(
        state.billing_webhook_idempotency_repository.clone(),
        state.billing_audit_log_repository.clone(),
        state.subscription_repository.clone(),
    );

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
pub fn verify_signature(secret: &str, headers: &HeaderMap, body: &[u8]) -> Result<(), ApiError> {
    let signature_header = headers
        .get("x-billing-signature")
        .or_else(|| headers.get("x-webhook-signature"))
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing billing webhook signature"))?;

    let expected_hex = signature_header
        .strip_prefix("sha256=")
        .ok_or_else(|| ApiError::unauthorized("invalid billing webhook signature format"))?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| ApiError::internal("failed to initialize billing webhook verifier"))?;
    mac.update(body);
    let actual_hex = hex::encode(mac.finalize().into_bytes());

    if actual_hex != expected_hex {
        return Err(ApiError::unauthorized("invalid billing webhook signature"));
    }

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
}
