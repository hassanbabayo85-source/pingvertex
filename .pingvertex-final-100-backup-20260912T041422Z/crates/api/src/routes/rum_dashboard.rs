use axum::{
    Router,
    routing::{get, post},
};

use crate::{
    handlers::{
        create_rum_site, list_rum_sites, rum_browser_device_breakdown, rum_error_breakdown,
        rum_geo_breakdown, rum_overview, rum_performance_trend, rum_session_stats, rum_slow_pages,
    },
    state::AppState,
};

/// Authenticated RUM dashboard routes. Nested behind `jwt_auth` in
/// `router.rs`, same as `dashboard::router()`.
///
/// Tenant isolation is NOT enforced here — every handler delegates to
/// `RumDashboardQueriesUseCase`/`RumSiteRepository`, which resolve
/// `site_id` against the caller's own `TenantContext` before touching
/// any RUM data (see `RumDashboardQueriesUseCase`'s doc comment). This
/// router only wires paths to handlers.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/dashboard/rum/sites",
            post(create_rum_site).get(list_rum_sites),
        )
        .route("/dashboard/rum/sites/{site_id}/overview", get(rum_overview))
        .route("/dashboard/rum/sites/{site_id}/pages", get(rum_slow_pages))
        .route(
            "/dashboard/rum/sites/{site_id}/errors",
            get(rum_error_breakdown),
        )
        .route(
            "/dashboard/rum/sites/{site_id}/browsers-devices",
            get(rum_browser_device_breakdown),
        )
        .route("/dashboard/rum/sites/{site_id}/geo", get(rum_geo_breakdown))
        .route(
            "/dashboard/rum/sites/{site_id}/trend",
            get(rum_performance_trend),
        )
        .route(
            "/dashboard/rum/sites/{site_id}/sessions",
            get(rum_session_stats),
        )
}
