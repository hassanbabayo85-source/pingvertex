use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    handlers::{get_proxy_check_job, submit_proxy_check_job},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/proxy-checks", post(submit_proxy_check_job))
        .route("/proxy-checks/{id}", get(get_proxy_check_job))
}
