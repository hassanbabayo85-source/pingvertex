use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/pricing/quote",
        post(crate::handlers::pricing_quote::pricing_quote),
    )
}

/// Admin-only pricing configuration routes.
pub fn admin_router() -> Router<AppState> {
    Router::new()
        // Order matters: /active must be defined before /{version} to avoid capture.
        .route(
            "/pricing/configs/active",
            get(crate::handlers::admin_pricing::get_active_pricing_config),
        )
        .route(
            "/pricing/configs",
            get(crate::handlers::admin_pricing::list_pricing_configs)
                .post(crate::handlers::admin_pricing::create_pricing_config),
        )
        .route(
            "/pricing/configs/{version}",
            get(crate::handlers::admin_pricing::get_pricing_config_by_version),
        )
        .route(
            "/pricing/audit-logs",
            get(crate::handlers::admin_pricing::list_pricing_audit_logs),
        )
}
