use axum::{Router, routing::patch};

use crate::handlers::update_subscription_usage;
use crate::state::AppState;

/// H12 — Proration Service: authenticated route for a tenant to change
/// its subscription's usage snapshot (monitors/interval/retention)
/// mid-cycle. Requires `Permission::ManageBilling`, checked inside the
/// handler — same pattern as `admin_pricing`'s `require_manage_billing`.
pub fn router() -> Router<AppState> {
    Router::new().route("/subscription/usage", patch(update_subscription_usage))
}
