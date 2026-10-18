use application::ports::PUBLIC_STATUS_LIST_CACHE_KEY;
use axum::{
    http::header,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use tracing::info;

use crate::{handlers::types::PublicMonitorResponse, state::AppState};

/// TTL for the cached `GET /public/status` (list) response. Shorter
/// than the per-status-page cache's 45s (see
/// `handlers::public_status_page::render_public_status_page`) since this
/// endpoint aggregates every public monitor across the whole deployment
/// rather than one tenant's hand-picked subset, so a slightly fresher
/// TTL was chosen as a reasonable default; both are additionally
/// invalidated immediately on incident open/resolve when the worker has
/// cache invalidation configured (see `worker::IncidentManager`).
const PUBLIC_STATUS_LIST_CACHE_TTL_SECONDS: u64 = 30;

/// Returns operational status for monitors explicitly marked `is_public`.
///
/// This endpoint is intentionally unauthenticated, so it must NEVER return
/// `user_id`, `assertions`, or any other owner-only/internal data — see
/// `PublicMonitorResponse` for the exact (safe) shape returned.
///
/// Now cache-backed via `state.status_page_cache`, the same cache
/// already used by `render_public_status_page`. Previously this handler
/// queried Postgres on every single request — `find_public()` plus a
/// `list_by_monitor` for checks AND incidents AND maintenance windows PER
/// MONITOR, unbounded, on an unauthenticated, publicly-linked endpoint.
/// Cache misses/errors are non-fatal, matching every other cache call
/// site in this codebase: a `get`/`set` failure just means "compute (and
/// don't cache) the response this time", never a failed request.
pub async fn get_public_status(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Response {
    info!("API: get_public_status");

    if let Ok(Some(cached)) = state
        .status_page_cache
        .get(PUBLIC_STATUS_LIST_CACHE_KEY)
        .await
    {
        return ([(header::CONTENT_TYPE, "application/json")], cached).into_response();
    }

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

    let body = match serde_json::to_string(&responses) {
        Ok(body) => body,
        Err(_) => {
            // Fall back to the plain (uncached) JSON response rather than
            // failing the request over a serialization edge case that
            // would also just fail identically on a retry.
            return axum::Json(responses).into_response();
        }
    };

    let _ = state
        .status_page_cache
        .set(
            PUBLIC_STATUS_LIST_CACHE_KEY,
            &body,
            PUBLIC_STATUS_LIST_CACHE_TTL_SECONDS,
        )
        .await;

    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}
