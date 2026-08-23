use axum::{Router, routing::get};

use crate::{handlers::list_checks, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/checks", get(list_checks))
}
