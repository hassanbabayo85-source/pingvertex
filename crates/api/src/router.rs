use axum::{
    middleware::{from_fn, from_fn_with_state},
    routing::get,
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::Level;

use crate::{
    auth_state::AuthState,
    metrics::render_metrics,
    middleware::{jwt_auth::jwt_auth, metrics::record_http_metrics},
    routes::{auth, checks, health, monitors},
    state::AppState,
};

pub fn build_router(app_state: AppState, auth_state: AuthState) -> Router {
    let health_routes = health::router().with_state(app_state.clone());
    let monitor_routes = monitors::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());
    let check_routes = checks::router().with_state(app_state.clone());

    let auth_routes = auth::router().with_state(auth_state.clone());

    Router::new()
        .route("/metrics", get(|| async { render_metrics() }))
        .merge(health_routes)
        .nest("/api/v1", monitor_routes)
        .nest("/api/v1", check_routes)
        .nest("/api/v1/auth", auth_routes) // Wannan zai haɗa su daidai da /api/v1/auth/register
        .layer(from_fn(record_http_metrics))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                ),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}
