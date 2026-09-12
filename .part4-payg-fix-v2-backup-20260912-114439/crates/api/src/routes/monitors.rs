use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post, put},
};
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};

use crate::{
    handlers::{
        acknowledge_incident,
        advanced_monitoring::{
            list_escalation_policies, list_maintenance_windows, update_escalation_policy,
        },
        bulk_monitor_action, confirm_bulk_monitor_import, create_escalation_policy,
        create_maintenance_window, create_monitor, create_one_time_check, delete_monitor,
        disable_monitor, enable_monitor, get_bulk_monitor_import_job, get_incident, get_monitor,
        get_one_time_check, list_incidents, list_monitor_checks, list_monitors,
        pause_bulk_monitor_import, preview_bulk_monitor_import, resume_bulk_monitor_import,
        update_monitor,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    let bulk_import = Router::new()
        .route(
            "/monitors/bulk-import/preview",
            post(preview_bulk_monitor_import),
        )
        .route(
            "/monitors/bulk-import/confirm",
            post(confirm_bulk_monitor_import),
        )
        .route(
            "/monitors/bulk-import/jobs/{job_id}",
            get(get_bulk_monitor_import_job),
        )
        .route(
            "/monitors/bulk-import/jobs/{job_id}/pause",
            post(pause_bulk_monitor_import),
        )
        .route(
            "/monitors/bulk-import/jobs/{job_id}/resume",
            post(resume_bulk_monitor_import),
        )
        .layer(DefaultBodyLimit::max(1_100_000))
        .layer(GovernorLayer::new(
            GovernorConfigBuilder::default()
                .per_second(2)
                .burst_size(4)
                .finish()
                .expect("valid bulk import rate-limit configuration"),
        ));

    Router::new()
        .route("/monitors", post(create_monitor).get(list_monitors))
        .route("/monitors/bulk", post(bulk_monitor_action))
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
        .route("/incidents/{incident_id}", get(get_incident))
        .route(
            "/incidents/{incident_id}/acknowledge",
            post(acknowledge_incident),
        )
        .merge(bulk_import)
}

pub fn one_time_check_router() -> Router<AppState> {
    Router::new()
        .route(
            "/monitors/{id}/checks/one-time",
            post(create_one_time_check),
        )
        .route(
            "/monitors/{id}/checks/one-time/{request_id}",
            get(get_one_time_check),
        )
}
