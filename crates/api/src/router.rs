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
        jwt_auth::jwt_auth, metrics::record_http_metrics, origin_check::require_matching_origin,
        require_recent_mfa::require_recent_mfa,
    },
    routes::{
        admin, api_flows, api_keys, audit, auth, billing, checks, dashboard, health, help,
        internal_agents, invitations, members, monitors, notifications, pricing, proxy_checks,
        public, rum_dashboard, status_pages, subscription, support, wallet,
    },
    state::AppState,
};

const MAX_REQUEST_BODY_SIZE: usize = 16 * 1024;

fn cors_layer() -> CorsLayer {
    let allowed_origin = crate::config::cors_allowed_origin_from_env();

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
            axum::http::header::HeaderName::from_static("idempotency-key"),
            axum::http::header::HeaderName::from_static("x-bulk-pricing-snapshot"),
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

    let one_time_check_routes = monitors::one_time_check_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(1)
                .burst_size(2)
                .finish()
                .expect("valid one-time check rate-limit configuration"),
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

    let dashboard_routes = dashboard::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid dashboard rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let dashboard_manual_check_routes = dashboard::manual_check_router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(1)
                .burst_size(2)
                .finish()
                .expect("valid dashboard manual-check rate-limit configuration"),
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
        .layer(DefaultBodyLimit::max(1_000_000))
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

    let rum_dashboard_routes = rum_dashboard::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid rum dashboard rate-limit configuration"),
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

    let wallet_routes = wallet::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(10)
                .finish()
                .expect("valid wallet rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Compatibility shell: the subscription handler is permanently retired
    // from the product model and returns HTTP 410.
    let subscription_routes = subscription::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(10)
                .finish()
                .expect("valid retired subscription rate-limit configuration"),
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
        // SECURITY: reject cross-origin login/registration/oauth/
        // password-reset submissions (see origin_check.rs doc comment).
        // Applied as its own middleware, in addition to (not instead of)
        // `cors_layer()`'s exact-origin allow-list.
        .layer(from_fn(require_matching_origin))
        .with_state(auth_state.clone());

    let totp_verify_rate_limit = GovernorConfigBuilder::default()
        .burst_size(5)
        .per_second(900)
        .finish()
        .expect("valid totp verify rate-limit configuration");

    let totp_verify_routes = auth::totp_verify_router()
        .layer(GovernorLayer::new(totp_verify_rate_limit))
        // SECURITY: same Origin check as `auth_routes` above — this is
        // the unauthenticated leg of a 2FA login (pending_token + code),
        // not yet behind `jwt_auth`.
        .layer(from_fn(require_matching_origin))
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

    // Public pricing quotes are deliberately read-only and rate-limited.
    // Actual PAYG previews and wallet mutations remain authenticated.
    let pricing_routes = pricing::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(30)
                .finish()
                .expect("valid pricing rate-limit configuration"),
        ))
        .with_state(app_state.clone());

    // Admin pricing routes: protected by JWT only; permission check is done
    // inside handlers (require ManageBilling, available to Owner only).
    // SECURITY: /admin pricing READS are gated on `jwt_auth` only.
    // WRITES are additionally gated on `require_recent_mfa` — a fresh
    // TOTP/backup-code step-up within RECENT_MFA_TTL_SECONDS — so a
    // plain session JWT can never change platform-wide pricing.
    let admin_pricing_reads = pricing::admin_router_reads()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(20)
                .finish()
                .expect("valid admin pricing read rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let admin_pricing_writes = pricing::admin_router_writes()
        .route_layer(from_fn_with_state(auth_state.clone(), require_recent_mfa))
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(1)
                .burst_size(5)
                .finish()
                .expect("valid admin pricing write rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let tenant_audit_routes = audit::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(5)
                .burst_size(10)
                .finish()
                .expect("valid tenant audit rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // SECURITY: platform-admin READS (list companies/users, audit
    // stream) require `jwt_auth` + `is_platform_admin()` only.
    // WRITES (suspend/activate company, suspend/delete user, wallet
    // adjustment, promotional credit) additionally require
    // `require_recent_mfa`: the platform-admin role is necessary but
    // not sufficient, so a stolen session cookie cannot perform
    // irreversible or financial operations.
    let platform_admin_reads = admin::router_reads()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(5)
                .burst_size(20)
                .finish()
                .expect("valid platform admin read rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let platform_admin_writes = admin::router_writes()
        .route_layer(from_fn_with_state(auth_state.clone(), require_recent_mfa))
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(1)
                .burst_size(5)
                .finish()
                .expect("valid platform admin write rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let help_routes = help::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(60)
                .finish()
                .expect("valid help rate-limit configuration"),
        ))
        .with_state(app_state.clone());

    let support_routes = support::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_millisecond(17)
                .burst_size(30)
                .finish()
                .expect("valid support rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    Router::new()
        .route("/metrics", get(metrics_handler))
        .merge(health_routes)
        .nest("/api/v1", monitor_routes)
        .nest("/api/v1", one_time_check_routes)
        .nest("/api/v1", check_routes)
        .nest("/api/v1", dashboard_routes)
        .nest("/api/v1", dashboard_manual_check_routes)
        .nest("/api/v1", notification_routes)
        .nest("/api/v1", notification_test_routes)
        .nest("/api/v1", api_flow_routes)
        .nest("/api/v1", proxy_check_routes)
        .nest("/api/v1", status_page_routes)
        .nest("/api/v1", rum_dashboard_routes)
        .nest("/api/v1", custom_domain_routes)
        .nest("/api/v1", billing_webhook_routes)
        .nest("/api/v1", billing_routes)
        .nest("/api/v1", subscription_routes)
        .nest("/api/v1", wallet_routes)
        .nest("/api/v1", pricing_routes)
        .nest("/api/v1/tenant", tenant_audit_routes)
        .nest("/api/v1", help_routes)
        .nest("/api/v1", support_routes)
        .nest("/api/v1/admin", admin_pricing_reads)
        .nest("/api/v1/admin", admin_pricing_writes)
        .nest("/api/v1/admin", platform_admin_reads)
        .nest("/api/v1/admin", platform_admin_writes)
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
                .make_span_with(RedactingMakeSpan)
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
    // PART 5 — agent_auth now needs `State<AppState>` (the agent
    // registry lives there), so it is wired via `from_fn_with_state`.
    let internal_agent_routes = internal_agents::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .key_extractor(AgentIdKeyExtractor)
                .per_millisecond(20)
                .burst_size(250)
                .finish()
                .expect("valid internal agent rate-limit configuration"),
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            agent_auth,
        ))
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

    let dashboard_routes = dashboard::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let dashboard_manual_check_routes = dashboard::manual_check_router()
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
        .layer(DefaultBodyLimit::max(1_000_000))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let status_page_routes = status_pages::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let rum_dashboard_routes = rum_dashboard::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let custom_domain_routes = status_pages::custom_domain_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let billing_routes = billing::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Compatibility shell: returns HTTP 410; no subscription mutation is possible.
    let subscription_routes = subscription::router()
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
        .route_layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            agent_auth,
        ))
        .with_state(app_state.clone());

    // NOTE (H10): deliberately still NO `GovernorLayer` here, consistent
    // with every other route in `build_test_router` — this test router
    // exists precisely so the E2E suite ISN'T subject to IP-based rate
    // limiting (which would make unrelated tests flaky/order-dependent
    // under `oneshot()`). Isolated rate-limit coverage for pricing lives in
    // `crates/api/tests/pricing_rate_limit.rs`, which uses the new
    // `build_pricing_only_router_with_rate_limit` below instead of this
    // function — mirroring how `build_auth_only_router_with_rate_limit`
    // already does the same for `auth_routes`.
    let pricing_routes = pricing::router().with_state(app_state.clone());

    let help_routes = help::router().with_state(app_state.clone());

    let support_routes = support::router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    let one_time_check_routes = monitors::one_time_check_router()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Admin pricing READS: JWT only (no MFA — reads are safe).
    let admin_pricing_reads = pricing::admin_router_reads()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Admin pricing WRITES: JWT + recent-MFA step-up.
    let admin_pricing_writes = pricing::admin_router_writes()
        .route_layer(from_fn_with_state(auth_state.clone(), require_recent_mfa))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());
    let tenant_audit_routes = audit::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(5)
                .burst_size(10)
                .finish()
                .expect("valid tenant audit rate-limit configuration"),
        ))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Platform-admin READS: JWT only.
    let platform_admin_reads = admin::router_reads()
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    // Platform-admin WRITES: JWT + recent-MFA step-up.
    let platform_admin_writes = admin::router_writes()
        .route_layer(from_fn_with_state(auth_state.clone(), require_recent_mfa))
        .route_layer(from_fn_with_state(auth_state.clone(), jwt_auth))
        .with_state(app_state.clone());

    Router::new()
        .route("/metrics", get(metrics_handler))
        .merge(health_routes)
        .nest("/api/v1", monitor_routes)
        .nest("/api/v1", one_time_check_routes)
        .nest("/api/v1", check_routes)
        .nest("/api/v1", dashboard_routes)
        .nest("/api/v1", dashboard_manual_check_routes)
        .nest("/api/v1", notification_routes)
        .nest("/api/v1", notification_test_routes)
        .nest("/api/v1", api_flow_routes)
        .nest("/api/v1", proxy_check_routes)
        .nest("/api/v1", status_page_routes)
        .nest("/api/v1", rum_dashboard_routes)
        .nest("/api/v1", custom_domain_routes)
        .nest("/api/v1", billing_webhook_routes)
        .nest("/api/v1", billing_routes)
        .nest("/api/v1", subscription_routes)
        .nest("/api/v1", pricing_routes)
        .nest("/api/v1/tenant", tenant_audit_routes)
        .nest("/api/v1", help_routes)
        .nest("/api/v1", support_routes)
        .nest("/api/v1/admin", admin_pricing_reads)
        .nest("/api/v1/admin", admin_pricing_writes)
        .nest("/api/v1/admin", platform_admin_reads)
        .nest("/api/v1/admin", platform_admin_writes)
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

/// H10: minimal router exposing ONLY `pricing_routes`, with the same
/// production `GovernorLayer` wired onto it in `build_router` above,
/// for isolated E2E rate-limit testing.
///
/// Mirrors `build_auth_only_router_with_rate_limit` immediately above it
/// (same shape, same reason for existing): `build_test_router` deliberately
/// omits `GovernorLayer` from every route group so the rest of the E2E
/// suite isn't flaky/order-dependent under IP-based rate limiting, which
/// means that shared harness structurally cannot be used to verify this
/// behavior. This constructor exists solely so
/// `crates/api/tests/pricing_rate_limit.rs` can exercise the real limiter
/// end-to-end without affecting any other test.
pub fn build_pricing_only_router_with_rate_limit(app_state: AppState) -> Router {
    let pricing_routes = pricing::router()
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(30)
                .finish()
                .expect("valid pricing rate-limit configuration"),
        ))
        .with_state(app_state);

    Router::new().nest("/api/v1", pricing_routes)
}


// =====================================================================
// PART 12 #7 — URI redaction for tracing spans
// =====================================================================

/// Custom `MakeSpan` for `TraceLayer` that redacts credential-bearing
/// path segments before they reach the tracing subscriber.
///
/// SECURITY: the default `DefaultMakeSpan` records the request URI in
/// the span, which would include the heartbeat token for
/// `GET /api/v1/public/heartbeat/{token}` — leaking the credential into
/// every log backend that ingests tracing spans. This wrapper preserves
/// the span's usefulness (method, version, path shape) while replacing
/// the token segment with `[REDACTED]`.
///
/// The new POST shape (`POST /api/v1/public/heartbeat`, token in a
/// header) is unaffected by this redactor because the token is never in
/// the URI in the first place.
#[derive(Clone, Copy, Debug)]
struct RedactingMakeSpan;

impl<B> tower_http::trace::MakeSpan<B> for RedactingMakeSpan {
    fn make_span(&mut self, request: &axum::http::Request<B>) -> tracing::Span {
        let uri = redact_uri_for_logging(request.uri());
        tracing::info_span!(
            "http.request",
            method = %request.method(),
            uri = %uri,
            version = ?request.version(),
        )
    }
}

/// Redacts credential-bearing path segments from a URI so it is safe to
/// record in a tracing span. Currently only the legacy heartbeat GET
/// path (`/api/v1/public/heartbeat/{token}`) carries a credential in the
/// URL.
fn redact_uri_for_logging(uri: &axum::http::Uri) -> String {
    let path = uri.path();
    if let Some(rest) = path.strip_prefix("/api/v1/public/heartbeat/") {
        if !rest.is_empty() && !rest.contains('/') {
            let redacted_path = "/api/v1/public/heartbeat/[REDACTED]";
            return match uri.query() {
                Some(q) => format!("{redacted_path}?{q}"),
                None => redacted_path.to_string(),
            };
        }
    }
    uri.to_string()
}
