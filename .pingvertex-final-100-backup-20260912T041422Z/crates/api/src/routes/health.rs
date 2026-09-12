use axum::{Router, routing::get};

use crate::handlers::health::{health, ready};
use crate::state::AppState;

/// Builds the health/readiness routes.
///
/// B34: `/ready` added alongside the existing `/health` — see the doc
/// comments on `handlers::health::health` / `handlers::health::ready` for
/// the distinction between the two.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
}
