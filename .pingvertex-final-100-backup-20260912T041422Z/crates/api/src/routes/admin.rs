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
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/audit", get(audit_records))
        .route("/companies", get(list_companies))
        .route("/users", get(list_users))
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
        .route("/companies/{id}/usage", get(company_usage))
}
