use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use application::dto::{
    BrowserDeviceBreakdown, GeoBreakdownRow, PagePerformanceRow, PerformanceTrendPoint,
    RumErrorBreakdown, RumOverview, RumTimeRange, SessionStats,
};
use application::use_cases::{
    CreateRumSiteError, CreateRumSiteUseCase, ListRumSitesUseCase, RumDashboardError,
    RumDashboardQueriesUseCase,
};
use domain::RumSite;
use domain::shared::types::{AuthenticatedUser, Permission};

use crate::error::ApiError;
use crate::handlers::pagination::{PaginationQuery, pagination_headers};
use crate::state::AppState;
use crate::validation::{parse_datetime, parse_rum_site_id};

/// Default lookback window when the caller omits `from`/`to` — 24 hours,
/// matching the dashboard's most common "last day" default view.
const DEFAULT_LOOKBACK_HOURS: i64 = 24;

/// Hard ceiling on how far back a single query is allowed to look and on
/// `limit`/`top_n` values, applied uniformly across every analytics
/// endpoint below. This exists to bound aggregate-query cost — without
/// it, a caller could request a multi-year range or an enormous `limit`
/// against a large `rum_page_views`/`rum_js_errors` table on every
/// request. 90 days mirrors a common "long-range" dashboard window;
/// widening it later is one constant to change, not a security
/// decision, so it isn't treated as one.
const MAX_RANGE_DAYS: i64 = 90;
const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 10;

fn map_dashboard_error(err: RumDashboardError) -> ApiError {
    match err {
        RumDashboardError::SiteNotFound => ApiError::not_found("RUM site not found"),
        RumDashboardError::Other(inner) => {
            ApiError::internal_with_source("failed to query RUM analytics", inner)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RumTimeRangeQuery {
    /// RFC 3339. Defaults to `now - 24h` if omitted.
    pub from: Option<String>,
    /// RFC 3339. Defaults to `now` if omitted.
    pub to: Option<String>,
}

impl RumTimeRangeQuery {
    fn resolve(self) -> Result<RumTimeRange, ApiError> {
        let now = Utc::now();

        let end = match self.to {
            Some(raw) => parse_datetime(&raw)?,
            None => now,
        };
        let start = match self.from {
            Some(raw) => parse_datetime(&raw)?,
            None => end - Duration::hours(DEFAULT_LOOKBACK_HOURS),
        };

        if start >= end {
            return Err(ApiError::bad_request("`from` must be before `to`"));
        }
        if end - start > Duration::days(MAX_RANGE_DAYS) {
            return Err(ApiError::bad_request(format!(
                "time range cannot exceed {MAX_RANGE_DAYS} days"
            )));
        }

        Ok(RumTimeRange { start, end })
    }
}

#[derive(Debug, Deserialize)]
pub struct RumLimitQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
}

impl RumLimitQuery {
    fn resolve(self) -> Result<(RumTimeRange, i64), ApiError> {
        let range = RumTimeRangeQuery {
            from: self.from,
            to: self.to,
        }
        .resolve()?;

        let limit = self.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(ApiError::bad_request(format!(
                "`limit` must be between 1 and {MAX_LIMIT}"
            )));
        }

        Ok((range, limit))
    }
}

#[derive(Debug, Serialize)]
pub struct RumSiteResponse {
    pub id: String,
    pub name: String,
    pub domain: String,
    /// Public, non-secret identifier — safe to return on every read, not
    /// just at creation (unlike an API key or webhook signing secret).
    /// See `RumSite::site_key`'s doc comment.
    pub site_key: String,
    pub created_at: DateTime<Utc>,
}

impl From<RumSite> for RumSiteResponse {
    fn from(site: RumSite) -> Self {
        Self {
            id: site.id.0.to_string(),
            name: site.name,
            domain: site.domain,
            site_key: site.site_key,
            created_at: site.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRumSiteBody {
    pub name: String,
    pub domain: String,
}

/// `POST /api/v1/dashboard/rum/sites`
pub async fn create_rum_site(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(body): Json<CreateRumSiteBody>,
) -> Result<(StatusCode, Json<RumSiteResponse>), ApiError> {
    // NOTE: `CreateRumSiteUseCase::execute` takes `TenantId` directly, not
    // `AuthenticatedUser`, so — unlike every other write use case in this
    // codebase — the permission check has to live here at the handler
    // layer instead of inside the use case. That's the exact pattern the
    // rest of this audit fix moved away from, because it means a second
    // caller of `CreateRumSiteUseCase` (a future internal job, a different
    // handler) would silently miss this check. Consider changing the use
    // case to accept `AuthenticatedUser` and call `.require(...)`
    // internally, matching `CreateMonitorUseCase` et al.
    if !authenticated_user.can(Permission::CreateMonitor) {
        return Err(ApiError::forbidden(
            "you do not have permission to create RUM sites",
        ));
    }

    let use_case = CreateRumSiteUseCase::new(state.rum_site_repository.clone());

    let site = use_case
        .execute(
            authenticated_user.tenant_context().tenant_id(),
            body.name,
            body.domain,
        )
        .await
        .map_err(|err| match err {
            CreateRumSiteError::Validation(inner) => ApiError::bad_request(inner.to_string()),
            CreateRumSiteError::Other(inner) => {
                ApiError::internal_with_source("failed to create RUM site", inner)
            }
        })?;

    Ok((StatusCode::CREATED, Json(RumSiteResponse::from(site))))
}

/// `GET /api/v1/dashboard/rum/sites?limit=50&cursor=<opaque>`
///
/// PART 8 — cursor-paginated. Response body is a JSON array (unchanged
/// from before PART 8); a next-page cursor (when present) is returned via
/// the `X-Next-Cursor` response header. Absence of the header = "last
/// page".
pub async fn list_rum_sites(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Query(query): Query<PaginationQuery>,
) -> Result<(HeaderMap, Json<Vec<RumSiteResponse>>), ApiError> {
    let use_case = ListRumSitesUseCase::new(state.rum_site_repository.clone());

    let page = use_case
        .execute_paginated(
            authenticated_user.tenant_context().tenant_id(),
            query.into_page_request(),
        )
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list RUM sites", err))?;

    let headers = pagination_headers(page.next_cursor)?;
    let body: Vec<RumSiteResponse> = page.items.into_iter().map(RumSiteResponse::from).collect();
    Ok((headers, Json(body)))
}

fn dashboard_queries(
    state: &AppState,
) -> RumDashboardQueriesUseCase<
    std::sync::Arc<dyn application::ports::RumSiteRepository>,
    std::sync::Arc<dyn application::ports::RumAnalyticsRepository>,
> {
    RumDashboardQueriesUseCase::new(
        state.rum_site_repository.clone(),
        state.rum_analytics_repository.clone(),
    )
}

/// `GET /api/v1/dashboard/rum/sites/{site_id}/overview`
pub async fn rum_overview(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(site_id): Path<String>,
    Query(query): Query<RumTimeRangeQuery>,
) -> Result<Json<RumOverview>, ApiError> {
    let site_id = parse_rum_site_id(&site_id)?;
    let range = query.resolve()?;

    let overview = dashboard_queries(&state)
        .overview(
            authenticated_user.tenant_context().tenant_id(),
            site_id,
            range,
        )
        .await
        .map_err(map_dashboard_error)?;

    Ok(Json(overview))
}

/// `GET /api/v1/dashboard/rum/sites/{site_id}/pages`
pub async fn rum_slow_pages(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(site_id): Path<String>,
    Query(query): Query<RumLimitQuery>,
) -> Result<Json<Vec<PagePerformanceRow>>, ApiError> {
    let site_id = parse_rum_site_id(&site_id)?;
    let (range, limit) = query.resolve()?;

    let rows = dashboard_queries(&state)
        .slow_pages(
            authenticated_user.tenant_context().tenant_id(),
            site_id,
            range,
            limit,
        )
        .await
        .map_err(map_dashboard_error)?;

    Ok(Json(rows))
}

/// `GET /api/v1/dashboard/rum/sites/{site_id}/errors`
pub async fn rum_error_breakdown(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(site_id): Path<String>,
    Query(query): Query<RumLimitQuery>,
) -> Result<Json<RumErrorBreakdown>, ApiError> {
    let site_id = parse_rum_site_id(&site_id)?;
    let (range, top_n) = query.resolve()?;

    let breakdown = dashboard_queries(&state)
        .error_breakdown(
            authenticated_user.tenant_context().tenant_id(),
            site_id,
            range,
            top_n,
        )
        .await
        .map_err(map_dashboard_error)?;

    Ok(Json(breakdown))
}

/// `GET /api/v1/dashboard/rum/sites/{site_id}/browsers-devices`
pub async fn rum_browser_device_breakdown(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(site_id): Path<String>,
    Query(query): Query<RumTimeRangeQuery>,
) -> Result<Json<BrowserDeviceBreakdown>, ApiError> {
    let site_id = parse_rum_site_id(&site_id)?;
    let range = query.resolve()?;

    let breakdown = dashboard_queries(&state)
        .browser_device_breakdown(
            authenticated_user.tenant_context().tenant_id(),
            site_id,
            range,
        )
        .await
        .map_err(map_dashboard_error)?;

    Ok(Json(breakdown))
}

/// `GET /api/v1/dashboard/rum/sites/{site_id}/geo`
///
/// Empty/all-`None`-country results are expected, not an error, until
/// geo-IP lookup is configured server-side — see Task E's `HANDOVER.md`
/// notes and `GeoBreakdownRow`'s doc comment.
pub async fn rum_geo_breakdown(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(site_id): Path<String>,
    Query(query): Query<RumLimitQuery>,
) -> Result<Json<Vec<GeoBreakdownRow>>, ApiError> {
    let site_id = parse_rum_site_id(&site_id)?;
    let (range, limit) = query.resolve()?;

    let rows = dashboard_queries(&state)
        .geo_breakdown(
            authenticated_user.tenant_context().tenant_id(),
            site_id,
            range,
            limit,
        )
        .await
        .map_err(map_dashboard_error)?;

    Ok(Json(rows))
}

/// `GET /api/v1/dashboard/rum/sites/{site_id}/trend`
pub async fn rum_performance_trend(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(site_id): Path<String>,
    Query(query): Query<RumTimeRangeQuery>,
) -> Result<Json<Vec<PerformanceTrendPoint>>, ApiError> {
    let site_id = parse_rum_site_id(&site_id)?;
    let range = query.resolve()?;

    let points = dashboard_queries(&state)
        .performance_trend(
            authenticated_user.tenant_context().tenant_id(),
            site_id,
            range,
        )
        .await
        .map_err(map_dashboard_error)?;

    Ok(Json(points))
}

/// `GET /api/v1/dashboard/rum/sites/{site_id}/sessions`
pub async fn rum_session_stats(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(site_id): Path<String>,
    Query(query): Query<RumTimeRangeQuery>,
) -> Result<Json<SessionStats>, ApiError> {
    let site_id = parse_rum_site_id(&site_id)?;
    let range = query.resolve()?;

    let stats = dashboard_queries(&state)
        .session_stats(
            authenticated_user.tenant_context().tenant_id(),
            site_id,
            range,
        )
        .await
        .map_err(map_dashboard_error)?;

    Ok(Json(stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(from: Option<&str>, to: Option<&str>) -> RumTimeRangeQuery {
        RumTimeRangeQuery {
            from: from.map(str::to_string),
            to: to.map(str::to_string),
        }
    }

    #[test]
    fn defaults_to_last_24_hours_when_omitted() {
        let range = query(None, None).resolve().unwrap();
        let hours = (range.end - range.start).num_hours();
        assert_eq!(hours, DEFAULT_LOOKBACK_HOURS);
    }

    #[test]
    fn rejects_from_after_to() {
        let err = query(Some("2026-01-02T00:00:00Z"), Some("2026-01-01T00:00:00Z"))
            .resolve()
            .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn rejects_range_wider_than_max() {
        let err = query(Some("2020-01-01T00:00:00Z"), Some("2026-01-01T00:00:00Z"))
            .resolve()
            .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn rejects_limit_out_of_bounds() {
        let err = RumLimitQuery {
            from: None,
            to: None,
            limit: Some(1000),
        }
        .resolve()
        .unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn accepts_limit_within_bounds() {
        let (_, limit) = RumLimitQuery {
            from: None,
            to: None,
            limit: Some(25),
        }
        .resolve()
        .unwrap();
        assert_eq!(limit, 25);
    }
}
