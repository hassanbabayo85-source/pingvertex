use axum::{Json, extract::State, response::IntoResponse};
use chrono::Utc;
use tracing::info;

use crate::{handlers::types::PublicMonitorResponse, state::AppState};

/// Returns operational status for monitors explicitly marked `is_public`.
///
/// This endpoint is intentionally unauthenticated, so it must NEVER return
/// `user_id`, `assertions`, or any other owner-only/internal data — see
/// `PublicMonitorResponse` for the exact (safe) shape returned.
pub async fn get_public_status(State(state): State<AppState>) -> impl IntoResponse {
    info!("API: get_public_status");

    let monitors = state
        .monitor_repository
        .find_public()
        .await
        .unwrap_or_default();

    let mut responses = Vec::with_capacity(monitors.len());
    let now = Utc::now();

    for monitor in monitors {
        let checks = state
            .check_repository
            .list_by_monitor(monitor.id)
            .await
            .unwrap_or_default();

        let incidents = state
            .incident_repository
            .list_by_monitor(monitor.id)
            .await
            .unwrap_or_default();

        let maintenance_windows = state
            .maintenance_window_repository
            .list_active_by_monitor(monitor.id, now)
            .await
            .unwrap_or_default();

        responses.push(PublicMonitorResponse::from_parts(
            monitor,
            &checks,
            &incidents,
            &maintenance_windows,
        ));
    }

    Json(responses)
}
