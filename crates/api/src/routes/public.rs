use axum::{
    Router,
    http::{HeaderValue, header},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use axum::extract::Request;

use crate::{
    handlers::{
        collect_rum_event, get_public_status, get_public_status_page, record_heartbeat,
        record_heartbeat_post,
    },
    state::AppState,
};

/// Unauthenticated routes. Never nest this behind `jwt_auth`.
///
/// `/heartbeat` (POST) and `/heartbeat/{token}` (GET, legacy) carry their
/// own per-token rate limit (see `handlers::heartbeat`), on top of the
/// IP-based `GovernorLayer` already applied to this whole router in
/// `router.rs`.
///
/// `/rum/collect` similarly carries its own per-site rate limit (see
/// `handlers::rum_ingest`) on top of the same IP-based `GovernorLayer`.
///
/// SECURITY (PART 12 #8): every response from this route group advertises
/// `Referrer-Policy: no-referrer`. This is the client-side half of the
/// "heartbeat credentials should not live permanently in URLs" invariant:
/// if the legacy GET URL (which carries the token in its path) is ever
/// loaded in a browser, the browser will not forward that URL as a
/// `Referer` header when the user clicks away from the resulting page,
/// so the token cannot leak into third-party logs or analytics via that
/// channel.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_public_status))
        .route("/status/{subdomain}", get(get_public_status_page))
        // RECOMMENDED: token in a header, never in the URL.
        .route("/heartbeat", post(record_heartbeat_post))
        // LEGACY: kept for backwards compatibility only.
        .route("/heartbeat/{token}", get(record_heartbeat))
        .route("/rum/collect", post(collect_rum_event))
        .layer(middleware::from_fn(set_referrer_policy_no_referrer))
}

/// PART 12 #8 — sets `Referrer-Policy: no-referrer` on every response
/// from `public::router()`.
///
/// Implemented as a tiny `from_fn` middleware rather than
/// `tower_http::set_header::SetResponseHeaderLayer`, because the
/// `set-header` feature of `tower-http` is not enabled in this crate's
/// `Cargo.toml` and adding a feature flag for one static header would be
/// heavier than the ten lines below.
async fn set_referrer_policy_no_referrer(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}
