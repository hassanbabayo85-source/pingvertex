use crate::{
    handlers::{
        cancel_proxy_check_job, get_proxy_check_job, list_proxy_check_results,
        preview_proxy_check_job, submit_proxy_check_job,
    },
    state::AppState,
};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Static route BEFORE the dynamic `{id}` route. Axum/matchit handles
        // precedence correctly regardless of registration order, but being
        // explicit avoids future confusion if the routing tree ever changes.
        .route("/proxy-checks/preview", post(preview_proxy_check_job))
        .route("/proxy-checks", post(submit_proxy_check_job))
        .route("/proxy-checks/{id}", get(get_proxy_check_job))
        .route("/proxy-checks/{id}/results", get(list_proxy_check_results))
        .route("/proxy-checks/{id}/cancel", post(cancel_proxy_check_job))
}
