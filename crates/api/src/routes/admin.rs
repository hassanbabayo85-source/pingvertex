use crate::{
    handlers::{
        adjust_company_wallet, audit_records, company_usage, grant_company_promo, list_companies,
        list_users, set_company_status, set_user_status,
    },
    state::AppState,
};
use axum::{
    Router,
    routing::{get, post},
};

/// Read-only platform-admin routes. JWT + platform-admin gate only —
/// no MFA step-up, because reading company/user lists and audit
/// streams is not a high-risk mutation.
pub fn router_reads() -> Router<AppState> {
    Router::new()
        .route("/audit", get(audit_records))
        .route("/companies", get(list_companies))
        .route("/users", get(list_users))
        .route("/companies/{id}/usage", get(company_usage))
}

/// Mutating platform-admin routes. Every one of these is a high-risk
/// operation (suspend/activate company, suspend/delete user, adjust
/// wallet, grant promotional credit) and therefore requires a fresh
/// MFA step-up IN ADDITION to the platform-admin role. The
/// `require_recent_mfa` middleware that enforces this is layered in
/// `router.rs`.
pub fn router_writes() -> Router<AppState> {
    Router::new()
        .route("/companies/{id}/status", post(set_company_status))
        .route("/users/{id}/status", post(set_user_status))
        .route(
            "/companies/{id}/wallet-adjustment",
            post(adjust_company_wallet),
        )
        .route(
            "/companies/{id}/promotional-credit",
            post(grant_company_promo),
        )
}
