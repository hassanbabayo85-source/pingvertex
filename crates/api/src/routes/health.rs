use axum::{Router, routing::get};

use crate::handlers::health;
use crate::state::AppState;

/// Builds the health route.
pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}
