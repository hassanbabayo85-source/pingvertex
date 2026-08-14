use axum::{
    extract::DefaultBodyLimit,
    middleware::{from_fn, from_fn_with_state},
    routing::get,
    Router,
};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::Level;

use crate::{
    auth_state::AuthState,
    metrics::metrics_handler,
    middleware::{jwt_auth::jwt_auth, metrics::record_http_metrics},
    routes::{api_flows, auth, checks, health, monitors, notifications, public},
    state::AppState,
};

// Iyakar girman JSON/request body a matakin GLOBAL na server, domin toshe
// DoS ta hanyar turo babban payload (misali multi-MB) zuwa ko wane
// endpoint, wanda zai sa server ya karanta/serialize/deserialize bayanai
// marasa amfani kafin ma a kai ga business logic.
//
// 16KB: `login`/`register` suna karɓan ƴan filaye ne kawai (ƙanana sosai),
// amma `create_monitor` yana karɓan `Vec<AssertionInput>` wanda zai iya
// ƙunshi daruruwan assertions na gaskiya a monitor guda ɗaya — 16KB yana
// isar da wannan amfani na gaskiya yayin da yake toshe manya-manyan
// payload marasa amfani.
const MAX_REQUEST_BODY_SIZE: usize = 16 * 1024;

fn cors_layer() -> CorsLayer {
    let allowed_origin = std::env::var("CORS_ALLOWED_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    CorsLayer::new()
        .allow_origin(AllowOrigin::exact(
            allowed_origin
                .parse()
                .expect("CORS_ALLOWED_ORIGIN must be a valid HeaderValue"),
        ))
        .allow_credentials(true) // domin browser ya aika/karba access_token cookie
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ])
}

/// Production router.
///
/// This includes the authentication rate limiter.
pub fn build_router(app_state: AppState, auth_state: AuthState) -> Router {
    let health_routes = health::router().with_state(app_state.clone());

    // Rate-limit authenticated, write-heavy endpoints (monitors, checks) by
    // client IP. This is a coarser limit than the auth limiter — it exists
    // to stop a single logged-in user from hammering the API (e.g. spamming
    // `POST /monitors`) and exhausting server/worker resources.
    //
    // 30 requests per 60 seconds, with a burst of 30.
    let monitor_routes = monitors::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(30)
                .use_headers()
                .finish()
                .expect("valid api rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let check_routes = checks::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(30)
                .use_headers()
                .finish()
                .expect("valid api rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Same standard write limit as monitors/checks — creating, listing, or
    // deleting a notification channel is no more sensitive than any other
    // monitor write.
    let notification_routes = notifications::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(30)
                .use_headers()
                .finish()
                .expect("valid api rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // A MUCH stricter limit than ordinary writes: "send test notification"
    // makes this server issue an outbound request (email or webhook) to a
    // destination the caller controls. Without a tight limit, a malicious
    // authenticated user could point a webhook channel at a third party's
    // URL and hammer this endpoint to use uptime-monitor as a spam/DoS
    // source against it.
    //
    // 5 requests per 60 seconds, burst of 5 — matches the auth endpoints'
    // limiter, which is the tightest precedent already in this codebase.
    let notification_test_routes = notifications::test_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(5)
                .use_headers()
                .finish()
                .expect("valid notification test rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // API Flow routes — the tightest limit in the API. One flow can run up
    // to 10 outbound HTTP steps on the worker, which is far more
    // resource-intensive than the single outbound request made by
    // "send test notification". This limit is deliberately stricter than
    // notification_test_routes (burst of 3 vs 5) to prevent an
    // authenticated user from converting the worker into an HTTP flooder.
    //
    // 3 requests per 60 seconds, burst of 3.
    let api_flow_routes = api_flows::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(3)
                .use_headers()
                .finish()
                .expect("valid api flow rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Rate-limit authentication endpoints by client IP.
    //
    // 5 requests per 60 seconds, with a burst of 5.
    // This limiter is shared by both /login and /register.
    let auth_rate_limit = GovernorConfigBuilder::default()
        .per_second(60)
        .burst_size(5)
        .use_headers()
        .finish()
        .expect("valid auth rate-limit configuration");

    let auth_routes = auth::router()
        .layer(GovernorLayer::new(auth_rate_limit))
        .with_state(auth_state.clone());

    // Sabon public status endpoint — BABU jwt_auth layer, domin an tsara shi
    // domin ya dawo da kawai monitors da aka yiwa alama a matsayin `is_public`.
    //
    // Ko da yake babu authentication, endpoint din yana yin N+1 queries
    // (checks + incidents ga kowane public monitor) a kowace kira, don haka
    // muna sanya IP-based rate limit a nan domin toshe resource-exhaustion
    // / DoS ta hanyar buƙatu masu yawa.
    //
    // 60 requests per 60 seconds, with a burst of 60.
    let public_routes = public::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(60)
                .use_headers()
                .finish()
                .expect("valid public rate-limit configuration"),
        ))
        .with_state(app_state.clone());

    Router::new()
        .route("/metrics", get(metrics_handler))
        .merge(health_routes)
        .nest("/api/v1", monitor_routes)
        .nest("/api/v1", check_routes)
        .nest("/api/v1", notification_routes)
        .nest("/api/v1", notification_test_routes)
        .nest("/api/v1", api_flow_routes)
        .nest("/api/v1/auth", auth_routes)
        .nest("/api/v1/public", public_routes)
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
        .layer(cors_layer())
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_SIZE))
}

/// Test router.
///
/// This is intentionally identical to the production router except that
/// the authentication rate limiter is disabled. Integration tests using
/// `tower::ServiceExt::oneshot()` do not provide a client SocketAddr,
/// which causes tower-governor's default IP extractor to fail.
pub fn build_test_router(app_state: AppState, auth_state: AuthState) -> Router {
    let health_routes = health::router().with_state(app_state.clone());

    let monitor_routes = monitors::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let check_routes = checks::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // No GovernorLayer here — same reasoning as monitor_routes/check_routes
    // above (tower-governor's IP extractor doesn't work under `oneshot()`).
    let notification_routes = notifications::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let notification_test_routes = notifications::test_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let api_flow_routes = api_flows::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // No GovernorLayer here.
    let auth_routes = auth::router().with_state(auth_state);

    let public_routes = public::router().with_state(app_state.clone());

    Router::new()
        .route("/metrics", get(metrics_handler))
        .merge(health_routes)
        .nest("/api/v1", monitor_routes)
        .nest("/api/v1", check_routes)
        .nest("/api/v1", notification_routes)
        .nest("/api/v1", notification_test_routes)
        .nest("/api/v1", api_flow_routes)
        .nest("/api/v1/auth", auth_routes)
        .nest("/api/v1/public", public_routes)
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
        .layer(cors_layer())
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_SIZE))
}
