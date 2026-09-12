use crate::{
    handlers::{
        cancel_proxy_check_job, get_proxy_check_job, list_proxy_check_results,
        submit_proxy_check_job,
    },
    state::AppState,
};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/proxy-checks", post(submit_proxy_check_job))
        .route("/proxy-checks/{id}", get(get_proxy_check_job))
        .route("/proxy-checks/{id}/results", get(list_proxy_check_results))
        .route("/proxy-checks/{id}/cancel", post(cancel_proxy_check_job))
}
