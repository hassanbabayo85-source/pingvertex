//! HTTP routes for recurring proxy monitors.

use axum::{
    Router,
    routing::{get, post},
};

use crate::{
    handlers::{
        create_recurring_proxy_monitor, delete_recurring_proxy_monitor,
        disable_recurring_proxy_monitor, enable_recurring_proxy_monitor,
        get_latest_recurring_proxy_check, get_recurring_proxy_monitor,
        list_recurring_proxy_checks, list_recurring_proxy_monitors,
        update_recurring_proxy_monitor,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Static route before the `{id}` route (axum handles precedence
        // correctly, but keeping the static-first style matches the
        // existing proxy_checks router).
        .route(
            "/proxy-monitors",
            post(create_recurring_proxy_monitor).get(list_recurring_proxy_monitors),
        )
        .route(
            "/proxy-monitors/{id}",
            get(get_recurring_proxy_monitor)
                .patch(update_recurring_proxy_monitor)
                .delete(delete_recurring_proxy_monitor),
        )
        .route(
            "/proxy-monitors/{id}/enable",
            post(enable_recurring_proxy_monitor),
        )
        .route(
            "/proxy-monitors/{id}/disable",
            post(disable_recurring_proxy_monitor),
        )
        .route(
            "/proxy-monitors/{id}/checks/latest",
            get(get_latest_recurring_proxy_check),
        )
        .route(
            "/proxy-monitors/{id}/checks",
            get(list_recurring_proxy_checks),
        )
}
