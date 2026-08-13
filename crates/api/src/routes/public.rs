use axum::{routing::get, Router};

use crate::{handlers::get_public_status, state::AppState};

/// Unauthenticated routes. Never nest this behind `jwt_auth`.
pub fn router() -> Router<AppState> {
    Router::new().route("/status", get(get_public_status))
}
