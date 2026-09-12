use axum::{
    Router,
    routing::{get, post},
};

use crate::{
    handlers::{collect_rum_event, get_public_status, get_public_status_page, record_heartbeat},
    state::AppState,
};

/// Unauthenticated routes. Never nest this behind `jwt_auth`.
///
/// `/heartbeat/{token}` additionally carries its OWN per-token rate limit
/// (see `handlers::heartbeat`), on top of the IP-based `GovernorLayer`
/// already applied to this whole router in `router.rs`.
///
/// `/rum/collect` similarly carries its own per-site rate limit (see
/// `handlers::rum_ingest`) on top of the same IP-based `GovernorLayer`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_public_status))
        .route("/status/{subdomain}", get(get_public_status_page))
        .route("/heartbeat/{token}", get(record_heartbeat))
        .route("/rum/collect", post(collect_rum_event))
}
