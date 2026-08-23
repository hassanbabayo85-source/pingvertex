use axum::{Router, routing::post};

use crate::handlers::{billing_webhook, initiate_checkout, verify_checkout};
use crate::state::AppState;

/// Authenticated billing routes (checkout initiation/verification).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/billing/checkout", post(initiate_checkout))
        .route("/billing/checkout/verify", post(verify_checkout))
}

/// Unauthenticated billing webhook route.
///
/// Signature verification is handled inside the handler itself; do NOT
/// apply JWT auth to this route.
pub fn webhook_router() -> Router<AppState> {
    Router::new().route("/billing/webhook", post(billing_webhook))
}
