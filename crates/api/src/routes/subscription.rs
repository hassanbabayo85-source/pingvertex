use axum::{Router, routing::patch};

use crate::handlers::update_subscription_usage;
use crate::state::AppState;

/// Compatibility shell for the retired subscription usage API. The handler
/// returns HTTP 410 and cannot mutate subscription billing.
pub fn router() -> Router<AppState> {
    Router::new().route("/subscription/usage", patch(update_subscription_usage))
}
