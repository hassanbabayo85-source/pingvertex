use axum::{
    extract::DefaultBodyLimit,
    routing::{post, put},
    Router,
};
use domain::LogoImage;

use crate::{
    handlers::{
        create_status_page, delete_status_page, list_status_pages, request_custom_domain,
        update_status_page, upload_status_page_logo,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/status-pages",
            post(create_status_page).get(list_status_pages),
        )
        .route(
            "/status-pages/{id}",
            put(update_status_page).delete(delete_status_page),
        )
        .route(
            "/status-pages/{id}/logo",
            post(upload_status_page_logo).layer(DefaultBodyLimit::max(LogoImage::MAX_SIZE)),
        )
}

/// Separate router for the custom-domain endpoint so it can be given its
/// own (tighter) rate-limit tier in `router.rs` — requesting a custom
/// domain kicks off a recurring background DNS verification job, unlike
/// ordinary status-page writes.
pub fn custom_domain_router() -> Router<AppState> {
    Router::new().route(
        "/status-pages/{id}/custom-domain",
        post(request_custom_domain),
    )
}
