use crate::{handlers::tenant_audit, state::AppState};
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new().route("/audit", get(tenant_audit))
}
