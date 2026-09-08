use axum::{
    Router,
    routing::{get, post},
};

use crate::handlers::billing_webhook;
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
        .route("/billing/invoices", get(crate::handlers::list_invoices))
        .route("/billing/invoices/{id}", get(crate::handlers::get_invoice))
        .route("/billing/usage", get(crate::handlers::wallet::usage_report))
        .route(
            "/billing/usage/summary",
            get(crate::handlers::wallet::usage_report),
        )
}

/// Unauthenticated billing webhook route.
///
/// Signature verification is handled inside the handler itself; do NOT
/// apply JWT auth to this route.
pub fn webhook_router() -> Router<AppState> {
    Router::new().route("/billing/webhook", post(billing_webhook))
}
