use axum::{
    Router,
    routing::{get, post},
};

use crate::handlers::{
    billing_summary, billing_webhook, create_add_on, initiate_checkout, verify_checkout,
};
use crate::state::AppState;

/// Authenticated billing routes (checkout initiation/verification).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/billing/checkout", post(initiate_checkout))
        .route("/billing/checkout/verify", post(verify_checkout))
        .route("/billing/addons", post(create_add_on))
        .route("/billing/summary", get(billing_summary))
}

/// Unauthenticated billing webhook route.
///
/// Signature verification is handled inside the handler itself; do NOT
/// apply JWT auth to this route.
pub fn webhook_router() -> Router<AppState> {
    Router::new().route("/billing/webhook", post(billing_webhook))
}
