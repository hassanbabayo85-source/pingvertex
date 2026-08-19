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
    middleware::{
        agent_auth::agent_auth, agent_id_key_extractor::AgentIdKeyExtractor,
        custom_domain_routing::custom_domain_routing, jwt_auth::jwt_auth,
        metrics::record_http_metrics,
    },
    routes::{
        api_flows, auth, checks, health, internal_agents, monitors, notifications, proxy_checks,
        public, status_pages,
    },
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
///
/// SECURITY NOTE (fixed): all GovernorLayers below now derive the rate-limit
/// key from the real TCP peer address (`ConnectInfo<SocketAddr>`), NOT from
/// client-supplied headers like `X-Forwarded-For` / `X-Real-IP`. There is no
/// reverse proxy in front of this API, so trusting those headers previously
/// let any caller bypass every rate limit by sending a different fake IP on
/// each request. This requires the server to be started with
/// `app.into_make_service_with_connect_info::<std::net::SocketAddr>()` in
/// `main.rs` — see the note there.
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
                .finish()
                .expect("valid api flow rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Proxy check routes — same strict tier as API Flow routes, because a
    // single proxy-check job can enqueue many outbound proxy checks on the
    // worker (similar resource risk as an API Flow's outbound steps).
    //
    // 3 requests per 60 seconds, burst of 3.
    let proxy_check_routes = proxy_checks::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(3)
                .finish()
                .expect("valid proxy check rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Status page CRUD — same standard write limit as monitors/checks.
    let status_page_routes = status_pages::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(30)
                .finish()
                .expect("valid status page rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Requesting a custom domain is much rarer and more expensive than an
    // ordinary status-page write: it kicks off a recurring background DNS
    // TXT-record verification job for as long as the domain stays
    // unverified. Tightened to the same tier as notification_test_routes
    // to prevent an authenticated user from spinning up an unbounded
    // number of these background jobs.
    //
    // 5 requests per 60 seconds, burst of 5.
    let custom_domain_routes = status_pages::custom_domain_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
                .burst_size(5)
                .finish()
                .expect("valid custom domain rate-limit configuration"),
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
        .finish()
        .expect("valid auth rate-limit configuration");

    let auth_routes = auth::router()
        .layer(GovernorLayer::new(auth_rate_limit))
        .with_state(auth_state.clone());

    // Sabon public status endpoint — BABU jwt_auth layer, domin an tsara shi
    // domin ya dawo da kawai monitors da aka yiwa alama a matsayin `is_public`
    // (ko monitors da aka zaɓa a wani status page na jama'a).
    //
    // Ko da yake babu authentication, endpoint din yana yin N+1 queries
    // (checks + incidents ga kowane public monitor) a kowace kira, don haka
    // muna sanya IP-based rate limit a nan domin toshe resource-exhaustion
    // / DoS ta hanyar buƙatu masu yawa. Status-page sashen yana da caching
    // a bayan fage domin rage wannan kaya har ma da wannan iyaka.
    //
    // 60 requests per 60 seconds, with a burst of 60.
    let public_routes = public::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(60)
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
        .layer(from_fn_with_state(app_state.clone(), custom_domain_routing))
}

/// Internal-agent mTLS router.
///
/// Serves ONLY `/internal/v1` (agent claim/submit endpoints), bound to a
/// separate `axum_server::bind_rustls` + `MtlsAcceptor` listener (see
/// `main.rs`), NOT the plain-HTTP `axum::serve` listener used by
/// `build_router()`. This split exists because `axum_server` does not
/// expose `ConnectInfo<SocketAddr>` (confirmed via source inspection of
/// axum-server 0.8.0), which the OTHER routes' IP-keyed GovernorLayers
/// depend on — migrating the whole server to `axum_server` would have
/// broken those.
///
/// No CORS layer (agents talk server-to-server, not from a browser).
///
/// Rate limit is keyed by *agent ID* (AgentIdKeyExtractor), not peer
/// IP — a generic IP-based GovernorLayer previously caused 429s under
/// normal Docker networking conditions, because all agents ended up
/// sharing the same (or an unstable) IP-derived bucket. Keying on
/// agent ID gives each agent its own independent quota, so one noisy
/// agent can't exhaust another agent's ability to claim/submit.
///
/// SECURITY NOTE (fixed): the previous version of this config also called
/// `.use_headers()` AFTER `.key_extractor(AgentIdKeyExtractor)`. In
/// tower-governor, whichever of `.key_extractor(...)` / `.use_headers()` /
/// `.use_peer_ip()` is called LAST wins, because they all write to the same
/// underlying field on the builder — so `.use_headers()` was silently
/// overriding `AgentIdKeyExtractor` and switching this limiter back to
/// reading the spoofable `X-Forwarded-For` / `X-Real-IP` headers. Removing
/// `.use_headers()` restores the intended agent-ID-keyed behavior described
/// above.
///
/// Sizing rationale (NOT a guess — derived from how claim_checks is
/// actually implemented today): `claim_checks` has no "due check"
/// filtering yet — it returns EVERY monitor matching the agent's
/// region on EVERY poll (see routes/internal_agents.rs). At a
/// realistic worst case of ~200 monitors in one region, one 5-second
/// poll cycle = 1 claim + up to 200 submit_result calls = ~201
/// requests / 5s ≈ 40 req/s sustained, per agent.
///
/// - burst_size 250: absorbs one full worst-case poll cycle (201
///   requests) plus headroom, even if the previous cycle's tokens
///   haven't fully replenished yet.
/// - per_millisecond(20): refills 1 token every 20ms = 50 tokens/sec
///   sustained, comfortably above the ~40 req/s worst case.
///
/// NOTE: if claim_checks later gains due-check filtering (returning
/// only monitors actually due for a check, not every matching
/// monitor every 5s), this quota can — and should — be tightened
/// significantly, since the worst case computed above will shrink.
///
/// TraceLayer / record_http_metrics are re-applied here (deliberately,
/// per explicit confirmation — this listener is on a separate `Router`
/// from `build_router()`'s, so it does NOT inherit those layers
/// automatically) to keep observability parity with the plain-HTTP
/// listener: agent traffic should show up in the same request logs and
/// `/metrics` counters as everything else.
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

    let proxy_check_routes = proxy_checks::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // No GovernorLayer here.
    let status_page_routes = status_pages::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let custom_domain_routes = status_pages::custom_domain_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let auth_routes = auth::router().with_state(auth_state);

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
        .nest("/api/v1/auth", auth_routes)
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
