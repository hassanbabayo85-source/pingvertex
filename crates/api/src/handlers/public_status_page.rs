use axum::{
    http::header,
    response::{IntoResponse, Response},
};
use chrono::Utc;

use crate::{error::ApiError, handlers::types::PublicStatusPageResponse, state::AppState};
use domain::entities::StatusPage;

/// Builds the public status-page HTTP response for an already-resolved
/// `StatusPage`.
///
/// Shared by both entry points that can reach a public status page:
/// `GET /api/v1/public/status/{subdomain}` and the host-header routing
/// middleware for verified custom domains. The cache key is the status
/// page's own id (not the subdomain or custom domain string), so a single
/// cache entry serves the same page regardless of which hostname the
/// visitor used to reach it.
pub async fn render_public_status_page(
    state: &AppState,
    status_page: StatusPage,
) -> Result<Response, ApiError> {
    let cache_key = format!("status_page:id:{}", status_page.id.0);

    if let Ok(Some(cached)) = state.status_page_cache.get(&cache_key).await {
        return Ok(([(header::CONTENT_TYPE, "application/json")], cached).into_response());
    }

    let mut monitors_out = Vec::with_capacity(status_page.monitor_ids.len());
    let now = Utc::now();

    for monitor_id in &status_page.monitor_ids {
        let Ok(Some(monitor)) = state.monitor_repository.find_by_id(*monitor_id).await else {
            continue;
        };

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

        monitors_out.push(crate::handlers::types::PublicMonitorResponse::from_parts(
            monitor,
            &checks,
            &incidents,
            &maintenance_windows,
        ));
    }

    let response = PublicStatusPageResponse {
        name: status_page.name.clone(),
        logo_url: status_page.logo_url.clone(),
        primary_color: status_page
            .primary_color
            .as_ref()
            .map(|color| color.as_str().to_owned()),
        monitors: monitors_out,
    };

    let body = serde_json::to_string(&response)
        .map_err(|err| ApiError::internal_with_source("failed to serialize status page", err))?;

    // Cache errors are non-fatal by design — never fail the request over
    // a caching problem.
    let _ = state.status_page_cache.set(&cache_key, &body, 45).await;

    Ok(([(header::CONTENT_TYPE, "application/json")], body).into_response())
}
