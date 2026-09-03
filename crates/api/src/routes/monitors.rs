use axum::{
    Router,
    routing::{get, post, put},
};

use crate::{
    handlers::{
        acknowledge_incident,
        advanced_monitoring::{
            list_escalation_policies, list_maintenance_windows, update_escalation_policy,
        },
        create_escalation_policy, create_maintenance_window, create_monitor, delete_monitor,
        disable_monitor, enable_monitor, get_monitor, list_incidents, list_monitor_checks,
        list_monitors, update_monitor,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/monitors", post(create_monitor).get(list_monitors))
        .route(
            "/monitors/{id}",
            get(get_monitor).put(update_monitor).delete(delete_monitor),
        )
        .route("/monitors/{id}/enable", post(enable_monitor))
        .route("/monitors/{id}/disable", post(disable_monitor))
        .route("/monitors/{id}/checks", get(list_monitor_checks))
        .route("/monitors/{id}/incidents", get(list_incidents))
        .route(
            "/monitors/{id}/maintenance-windows",
            post(create_maintenance_window).get(list_maintenance_windows),
        )
        .route(
            "/escalation-policies",
            post(create_escalation_policy).get(list_escalation_policies),
        )
        .route("/escalation-policies/{id}", put(update_escalation_policy))
        .route(
            "/incidents/{incident_id}/acknowledge",
            post(acknowledge_incident),
        )
}
