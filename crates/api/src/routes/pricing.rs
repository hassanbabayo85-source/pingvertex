use crate::{handlers::admin_pricing, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    // The old /pricing/quote was removed. This is now an empty router so the main router is not broken.
    Router::new()
}

/// Read-only admin pricing routes. JWT required, MFA step-up NOT
/// required — reads cannot change any platform state and are safe to
/// expose under the same session-reload gate as every other
/// authenticated read.
pub fn admin_router_reads() -> Router<AppState> {
    Router::new()
        // Order matters: /active must be defined before /{version} to avoid capture.
        .route(
            "/pricing/configs/active",
            get(admin_pricing::get_active_pricing_config),
        )
        .route("/pricing/configs", get(admin_pricing::list_pricing_configs))
        .route(
            "/pricing/configs/{version}",
            get(admin_pricing::get_pricing_config_by_version),
        )
        .route(
            "/pricing/audit-logs",
            get(admin_pricing::list_pricing_audit_logs),
        )
}

/// Mutating admin pricing routes. In addition to JWT, the caller must
/// have completed a TOTP/backup-code step-up within
/// `RECENT_MFA_TTL_SECONDS` — enforced by the `require_recent_mfa`
/// middleware layered in `router.rs`. This is the invariant of
/// the platform-admin hardening task requires: a plain session JWT
/// must never be sufficient to change platform-wide pricing.
pub fn admin_router_writes() -> Router<AppState> {
    Router::new()
        .route(
            "/pricing/configs",
            post(admin_pricing::create_pricing_config),
        )
        .route(
            "/pricing/payg-configs",
            post(admin_pricing::create_payg_price),
        )
}
