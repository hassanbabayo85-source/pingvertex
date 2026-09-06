use axum::{
    Router,
    routing::{get, post},
};

use crate::handlers::{
    billing_summary, billing_webhook, cancel_add_on, change_plan, create_add_on, initiate_checkout,
    list_invoices, verify_add_on, verify_checkout, verify_plan_change, get_subscription, cancel_subscription, list_plans, get_invoice,
};
use crate::state::AppState;

/// Authenticated billing routes (checkout initiation/verification).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/billing/subscription", get(get_subscription))
        .route("/billing/subscription/cancel", post(cancel_subscription))
        .route("/billing/plans", get(list_plans))
        .route("/payg/pricing", get(crate::handlers::payg_pricing::payg_pricing))
        .route("/billing/quote", post(crate::handlers::pricing_quote::pricing_quote_authenticated))
        .route("/billing/checkout", post(initiate_checkout))
        .route("/billing/checkout/verify", post(verify_checkout))
        .route("/billing/addons", post(create_add_on))
        .route("/billing/addons/verify", post(verify_add_on))
        .route("/billing/addons/cancel", post(cancel_add_on))
        .route("/billing/plan-change", post(change_plan))
        .route("/billing/plan-change/verify", post(verify_plan_change))
        .route("/billing/invoices", get(list_invoices))
        .route("/billing/invoices/{id}", get(get_invoice))
        .route("/billing/usage", get(crate::handlers::wallet::usage_report))
        .route("/billing/usage/summary", get(crate::handlers::wallet::usage_report))
        .route("/billing/summary", get(billing_summary))
}

/// Unauthenticated billing webhook route.
///
/// Signature verification is handled inside the handler itself; do NOT
/// apply JWT auth to this route.
pub fn webhook_router() -> Router<AppState> {
    Router::new().route("/billing/webhook", post(billing_webhook))
}

