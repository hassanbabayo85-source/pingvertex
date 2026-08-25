use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware::{from_fn, from_fn_with_state},
    routing::get,
};
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::LatencyUnit;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::{
    auth_state::AuthState,
    metrics::metrics_handler,
    middleware::{
        agent_auth::agent_auth, agent_id_key_extractor::AgentIdKeyExtractor,
        api_key_auth::api_key_auth, custom_domain_routing::custom_domain_routing,
        jwt_auth::jwt_auth, metrics::record_http_metrics,
    },
    routes::{
        api_flows, api_keys, auth, billing, checks, health, internal_agents, invitations, members,
        monitors, notifications, proxy_checks, public, status_pages,
    },
    state::AppState,
};

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
        .allow_credentials(true)
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

pub fn build_router(app_state: AppState, auth_state: AuthState) -> Router {
    let health_routes = health::router().with_state(app_state.clone());

    let monitor_routes = monitors::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid api rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let check_routes = checks::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid api rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let notification_routes = notifications::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid api rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let notification_test_routes = notifications::test_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(5)
                .finish()
                .expect("valid notification test rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let api_flow_routes = api_flows::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(3)
                .finish()
                .expect("valid api flow rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let proxy_check_routes = proxy_checks::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(3)
                .finish()
                .expect("valid proxy check rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let status_page_routes = status_pages::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid status page rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let custom_domain_routes = status_pages::custom_domain_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(5)
                .finish()
                .expect("valid custom domain rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let billing_routes = billing::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(10)
                .finish()
                .expect("valid billing rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let billing_webhook_routes = billing::webhook_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(10)
                .finish()
                .expect("valid billing webhook rate-limit configuration"),
        ))
        .with_state(app_state.clone());

    let auth_rate_limit = GovernorConfigBuilder::default()
        .per_second(60)
        .burst_size(5)
        .finish()
        .expect("valid auth rate-limit configuration");

    let auth_routes = auth::router()
        .layer(GovernorLayer::new(auth_rate_limit))
        .with_state(auth_state.clone());

    let totp_verify_rate_limit = GovernorConfigBuilder::default()
        .burst_size(5)
        .per_second(900)
        .finish()
        .expect("valid totp verify rate-limit configuration");

    let totp_verify_routes = auth::totp_verify_router()
        .layer(GovernorLayer::new(totp_verify_rate_limit))
        .with_state(auth_state.clone());

    let totp_protected_routes = auth::protected_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid totp management rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    let api_key_routes = api_keys::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid api key rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let membership_routes = members::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid membership rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    let invitation_routes = invitations::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid invitation rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    // NOTE: Scope enforcement now lives inside `api_keys::external_router()`
    // (per-route `require_api_key_scope`), not here. We only authenticate
    // via `api_key_auth` once for the whole external group.
    let external_routes = api_keys::external_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid external api-key rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(app_state.clone(), api_key_auth))
        .with_state(app_state.clone());

    let public_routes = public::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(60)
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
        .nest("/api/v1", proxy_check_routes)
        .nest("/api/v1", status_page_routes)
        .nest("/api/v1", custom_domain_routes)
        .nest("/api/v1", billing_webhook_routes)
        .nest("/api/v1", billing_routes)
        .nest("/api/v1", api_key_routes)
        .nest("/api/v1", membership_routes)
        .nest("/api/v1", invitation_routes)
        .nest("/api/v1", external_routes)
        .nest("/api/v1/auth", auth_routes)
        .nest("/api/v1/auth", totp_verify_routes)
        .nest("/api/v1/auth", totp_protected_routes)
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
        .layer(from_fn_with_state(app_state.clone(), custom_domain_routing))
}

pub fn build_internal_agent_router(app_state: AppState) -> Router {
    let internal_agent_routes = internal_agents::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .key_extractor(AgentIdKeyExtractor)
                .per_millisecond(20)
                .burst_size(250)
                .finish()
                .expect("valid internal agent rate-limit configuration"),
        ))
        .route_layer(axum::middleware::from_fn(agent_auth))
        .with_state(app_state.clone());

    Router::new()
        .nest("/internal/v1", internal_agent_routes)
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
}

pub fn build_test_router(app_state: AppState, auth_state: AuthState) -> Router {
    let health_routes = health::router().with_state(app_state.clone());

    let monitor_routes = monitors::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let check_routes = checks::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let notification_routes = notifications::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let notification_test_routes = notifications::test_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let api_flow_routes = api_flows::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let proxy_check_routes = proxy_checks::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let status_page_routes = status_pages::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let custom_domain_routes = status_pages::custom_domain_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let billing_routes = billing::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let billing_webhook_routes = billing::webhook_router().with_state(app_state.clone());

    let api_key_routes = api_keys::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let membership_routes = members::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    let invitation_routes = invitations::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    // Same as in `build_router`: scope enforcement is inside
    // `external_router()`. Only authentication is layered here.
    let external_routes = api_keys::external_router()
        .route_layer(from_fn_with_state(app_state.clone(), api_key_auth))
        .with_state(app_state.clone());

    let auth_routes = auth::router().with_state(auth_state.clone());

    let totp_verify_routes = auth::totp_verify_router().with_state(auth_state.clone());

    let totp_protected_routes = auth::protected_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(auth_state.clone());

    let public_routes = public::router().with_state(app_state.clone());

    let internal_agent_routes = internal_agents::router()
        .route_layer(axum::middleware::from_fn(agent_auth))
        .with_state(app_state.clone());

    Router::new()
        .route("/metrics", get(metrics_handler))
        .merge(health_routes)
        .nest("/api/v1", monitor_routes)
        .nest("/api/v1", check_routes)
        .nest("/api/v1", notification_routes)
        .nest("/api/v1", notification_test_routes)
        .nest("/api/v1", api_flow_routes)
        .nest("/api/v1", proxy_check_routes)
        .nest("/api/v1", status_page_routes)
        .nest("/api/v1", custom_domain_routes)
        .nest("/api/v1", billing_webhook_routes)
        .nest("/api/v1", billing_routes)
        .nest("/api/v1", api_key_routes)
        .nest("/api/v1", membership_routes)
        .nest("/api/v1", invitation_routes)
        .nest("/api/v1", external_routes)
        .nest("/api/v1/auth", auth_routes)
        .nest("/api/v1/auth", totp_verify_routes)
        .nest("/api/v1/auth", totp_protected_routes)
        .nest("/api/v1/public", public_routes)
        .nest("/internal/v1", internal_agent_routes)
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
        .layer(from_fn_with_state(app_state.clone(), custom_domain_routing))
}

pub fn build_auth_only_router_with_rate_limit(auth_state: AuthState) -> Router {
    let auth_rate_limit = GovernorConfigBuilder::default()
        .per_second(60)
        .burst_size(5)
        .finish()
        .expect("valid auth rate-limit configuration");

    let auth_routes = auth::router()
        .layer(GovernorLayer::new(auth_rate_limit))
        .with_state(auth_state);

    Router::new().nest("/api/v1/auth", auth_routes)
}
