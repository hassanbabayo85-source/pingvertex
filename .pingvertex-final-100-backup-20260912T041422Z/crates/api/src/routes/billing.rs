use axum::{
    Router,
    routing::{get, post},
};

use crate::handlers::billing_webhook;
use crate::handlers::paystack_webhook::paystack_webhook;
use crate::state::AppState;

/// PAYG-first authenticated billing surface. Historical subscription handlers are not routed here.
pub fn router() -> Router<AppState> {
    // PAYG-first runtime surface. Historical subscription handlers remain in
    // the binary for migration/audit compatibility but are no longer routed.
    Router::new()
        .route(
            "/payg/pricing",
            get(crate::handlers::payg_pricing::payg_pricing),
        )
        .route(
            "/payg/preview",
            axum::routing::post(crate::handlers::payg_preview::payg_preview),
        )
        .route("/billing/invoices", get(crate::handlers::list_invoices))
        .route("/billing/invoices/{id}", get(crate::handlers::get_invoice))
        .route("/billing/usage", get(crate::handlers::wallet::usage_report))
        .route(
            "/billing/usage/summary",
            get(crate::handlers::wallet::usage_report),
        )
}

/// Unauthenticated billing webhook routes.
///
/// Signature verification is handled inside each handler; do NOT apply JWT
/// auth to any route mounted here.
///
/// Two endpoints because the two providers differ on signature algorithm,
/// header name, and payload shape:
///   * `/billing/webhook`           — internal shared-secret contract
///     (HMAC-SHA256, `sha256=<hex>`).
///   * `/billing/webhook/paystack`  — Paystack's native contract
///     (HMAC-SHA512, `x-paystack-signature`).
pub fn webhook_router() -> Router<AppState> {
    Router::new()
        .route("/billing/webhook", post(billing_webhook))
        .route("/billing/webhook/paystack", post(paystack_webhook))
}
