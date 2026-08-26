use crate::state::AppState;
use axum::{Router, routing::post};

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/pricing/quote",
        post(crate::handlers::pricing_quote::pricing_quote),
    )
}
