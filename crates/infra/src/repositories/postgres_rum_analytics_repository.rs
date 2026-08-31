use std::error::Error;

use application::dto::{
    BrowserBreakdownRow, BrowserDeviceBreakdown, DeviceBreakdownRow, GeoBreakdownRow,
    PagePerformanceRow, PerformanceTrendPoint, RumErrorBreakdown, RumOverview, RumTimeRange,
    SessionStats, TopJsErrorRow, TopResourceErrorRow,
};
use application::ports::RumAnalyticsRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::{RumSiteId, TenantId};
use sqlx::{FromRow, PgPool};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresRumAnalyticsRepository {
    pool: PgPool,
}

impl PostgresRumAnalyticsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn error_rate(errors: i64, page_views: i64) -> Option<f64> {
    if page_views > 0 {
        Some(errors as f64 / page_views as f64)
    } else {
        None
    }
}

#[derive(Debug, FromRow)]
struct OverviewRow {
    page_view_count: i64,
    session_count: i64,
    avg_load_time_ms: Option<f64>,
    p95_load_time_ms: Option<f64>,
    p75_lcp_ms: Option<f64>,
    p75_inp_ms: Option<f64>,
    p75_cls: Option<f64>,
    js_error_count: i64,
    resource_error_count: i64,
}

#[derive(Debug, FromRow)]
struct PagePerformanceRowSql {
    path: String,
    view_count: i64,
    avg_load_time_ms: Option<f64>,
    p95_load_time_ms: Option<f64>,
    avg_lcp_ms: Option<f64>,
}

#[derive(Debug, FromRow)]
struct ErrorCountsRow {
    js_error_count: i64,
    resource_error_count: i64,
    page_view_count: i64,
}

#[derive(Debug, FromRow)]
struct TopJsErrorRowSql {
    message: String,
    occurrence_count: i64,
}

#[derive(Debug, FromRow)]
struct TopResourceErrorRowSql {
    resource_url: String,
    occurrence_count: i64,
}

#[derive(Debug, FromRow)]
struct BrowserBreakdownRowSql {
    browser_name: Option<String>,
    session_count: i64,
}

#[derive(Debug, FromRow)]
struct DeviceBreakdownRowSql {
    device_type: Option<String>,
    session_count: i64,
}

#[derive(Debug, FromRow)]
struct GeoBreakdownRowSql {
    country_code: Option<String>,
    session_count: i64,
}

#[derive(Debug, FromRow)]
struct PerformanceTrendRowSql {
    bucket_start: DateTime<Utc>,
    page_view_count: i64,
    avg_load_time_ms: Option<f64>,
    avg_lcp_ms: Option<f64>,
}

#[derive(Debug, FromRow)]
struct SessionStatsRow {
    session_count: i64,
    avg_page_views_per_session: Option<f64>,
    avg_session_duration_secs: Option<f64>,
}

#[async_trait]
impl RumAnalyticsRepository for PostgresRumAnalyticsRepository {
    async fn overview(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<RumOverview> {
        // Four source tables (page_views / sessions / js_errors /
        // resource_errors) can't share one GROUP BY, so this is composed
        // of independent scalar subqueries, each a single indexed
        // aggregate over (tenant_id, site_id, time range) — still one
        // round trip, still no N+1 (N is fixed at 9 subqueries
        // regardless of data volume, not per-row).
        let row = sqlx::query_as::<_, OverviewRow>(
            r#"
            SELECT
                (SELECT COUNT(*) FROM rum_page_views
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                ) AS page_view_count,
                (SELECT COUNT(*) FROM rum_sessions
                    WHERE tenant_id = $1 AND site_id = $2 AND started_at >= $3 AND started_at < $4
                ) AS session_count,
                (SELECT AVG(load_time_ms) FROM rum_page_views
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                ) AS avg_load_time_ms,
                (SELECT percentile_cont(0.95) WITHIN GROUP (ORDER BY load_time_ms) FROM rum_page_views
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                        AND load_time_ms IS NOT NULL
                ) AS p95_load_time_ms,
                (SELECT percentile_cont(0.75) WITHIN GROUP (ORDER BY lcp_ms) FROM rum_page_views
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                        AND lcp_ms IS NOT NULL
                ) AS p75_lcp_ms,
                (SELECT percentile_cont(0.75) WITHIN GROUP (ORDER BY inp_ms) FROM rum_page_views
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                        AND inp_ms IS NOT NULL
                ) AS p75_inp_ms,
                (SELECT percentile_cont(0.75) WITHIN GROUP (ORDER BY cls) FROM rum_page_views
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                        AND cls IS NOT NULL
                ) AS p75_cls,
                (SELECT COUNT(*) FROM rum_js_errors
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                ) AS js_error_count,
                (SELECT COUNT(*) FROM rum_resource_errors
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                ) AS resource_error_count
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .fetch_one(&self.pool)
        .await?;

        let total_errors = row.js_error_count + row.resource_error_count;

        Ok(RumOverview {
            page_view_count: row.page_view_count,
            session_count: row.session_count,
            avg_load_time_ms: row.avg_load_time_ms,
            p95_load_time_ms: row.p95_load_time_ms,
            p75_lcp_ms: row.p75_lcp_ms,
            p75_inp_ms: row.p75_inp_ms,
            p75_cls: row.p75_cls,
            js_error_count: row.js_error_count,
            resource_error_count: row.resource_error_count,
            error_rate: error_rate(total_errors, row.page_view_count),
        })
    }

    async fn slow_pages(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        limit: i64,
    ) -> RepoResult<Vec<PagePerformanceRow>> {
        let rows = sqlx::query_as::<_, PagePerformanceRowSql>(
            r#"
            SELECT
                path,
                COUNT(*) AS view_count,
                AVG(load_time_ms) AS avg_load_time_ms,
                percentile_cont(0.95) WITHIN GROUP (ORDER BY load_time_ms)
                    FILTER (WHERE load_time_ms IS NOT NULL) AS p95_load_time_ms,
                AVG(lcp_ms) AS avg_lcp_ms
            FROM rum_page_views
            WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
            GROUP BY path
            ORDER BY avg_load_time_ms DESC NULLS LAST
            LIMIT $5
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| PagePerformanceRow {
                path: r.path,
                view_count: r.view_count,
                avg_load_time_ms: r.avg_load_time_ms,
                p95_load_time_ms: r.p95_load_time_ms,
                avg_lcp_ms: r.avg_lcp_ms,
            })
            .collect())
    }

    async fn error_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        top_n: i64,
    ) -> RepoResult<RumErrorBreakdown> {
        let counts = sqlx::query_as::<_, ErrorCountsRow>(
            r#"
            SELECT
                (SELECT COUNT(*) FROM rum_js_errors
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                ) AS js_error_count,
                (SELECT COUNT(*) FROM rum_resource_errors
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                ) AS resource_error_count,
                (SELECT COUNT(*) FROM rum_page_views
                    WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                ) AS page_view_count
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .fetch_one(&self.pool)
        .await?;

        let top_js_errors = sqlx::query_as::<_, TopJsErrorRowSql>(
            r#"
            SELECT message, COUNT(*) AS occurrence_count
            FROM rum_js_errors
            WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
            GROUP BY message
            ORDER BY occurrence_count DESC
            LIMIT $5
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .bind(top_n)
        .fetch_all(&self.pool)
        .await?;

        let top_resource_errors = sqlx::query_as::<_, TopResourceErrorRowSql>(
            r#"
            SELECT resource_url, COUNT(*) AS occurrence_count
            FROM rum_resource_errors
            WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
            GROUP BY resource_url
            ORDER BY occurrence_count DESC
            LIMIT $5
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .bind(top_n)
        .fetch_all(&self.pool)
        .await?;

        let total_errors = counts.js_error_count + counts.resource_error_count;

        Ok(RumErrorBreakdown {
            js_error_count: counts.js_error_count,
            resource_error_count: counts.resource_error_count,
            page_view_count: counts.page_view_count,
            error_rate: error_rate(total_errors, counts.page_view_count),
            top_js_errors: top_js_errors
                .into_iter()
                .map(|r| TopJsErrorRow {
                    message: r.message,
                    occurrence_count: r.occurrence_count,
                })
                .collect(),
            top_resource_errors: top_resource_errors
                .into_iter()
                .map(|r| TopResourceErrorRow {
                    resource_url: r.resource_url,
                    occurrence_count: r.occurrence_count,
                })
                .collect(),
        })
    }

    async fn browser_device_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<BrowserDeviceBreakdown> {
        let browsers = sqlx::query_as::<_, BrowserBreakdownRowSql>(
            r#"
            SELECT browser_name, COUNT(*) AS session_count
            FROM rum_sessions
            WHERE tenant_id = $1 AND site_id = $2 AND started_at >= $3 AND started_at < $4
            GROUP BY browser_name
            ORDER BY session_count DESC
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .fetch_all(&self.pool)
        .await?;

        let devices = sqlx::query_as::<_, DeviceBreakdownRowSql>(
            r#"
            SELECT device_type, COUNT(*) AS session_count
            FROM rum_sessions
            WHERE tenant_id = $1 AND site_id = $2 AND started_at >= $3 AND started_at < $4
            GROUP BY device_type
            ORDER BY session_count DESC
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .fetch_all(&self.pool)
        .await?;

        Ok(BrowserDeviceBreakdown {
            browsers: browsers
                .into_iter()
                .map(|r| BrowserBreakdownRow {
                    browser_name: r.browser_name,
                    session_count: r.session_count,
                })
                .collect(),
            devices: devices
                .into_iter()
                .map(|r| DeviceBreakdownRow {
                    device_type: r.device_type,
                    session_count: r.session_count,
                })
                .collect(),
        })
    }

    async fn geo_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        limit: i64,
    ) -> RepoResult<Vec<GeoBreakdownRow>> {
        let rows = sqlx::query_as::<_, GeoBreakdownRowSql>(
            r#"
            SELECT country_code, COUNT(*) AS session_count
            FROM rum_sessions
            WHERE tenant_id = $1 AND site_id = $2 AND started_at >= $3 AND started_at < $4
            GROUP BY country_code
            ORDER BY session_count DESC
            LIMIT $5
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| GeoBreakdownRow {
                country_code: r.country_code,
                session_count: r.session_count,
            })
            .collect())
    }

    async fn performance_trend(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<Vec<PerformanceTrendPoint>> {
        let rows = sqlx::query_as::<_, PerformanceTrendRowSql>(
            r#"
            SELECT
                date_trunc('hour', created_at) AS bucket_start,
                COUNT(*) AS page_view_count,
                AVG(load_time_ms) AS avg_load_time_ms,
                AVG(lcp_ms) AS avg_lcp_ms
            FROM rum_page_views
            WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
            GROUP BY bucket_start
            ORDER BY bucket_start ASC
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| PerformanceTrendPoint {
                bucket_start: r.bucket_start,
                page_view_count: r.page_view_count,
                avg_load_time_ms: r.avg_load_time_ms,
                avg_lcp_ms: r.avg_lcp_ms,
            })
            .collect())
    }

    async fn session_stats(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<SessionStats> {
        // `avg_page_views_per_session` averages only over sessions that
        // have at least one page view in range (the LEFT JOIN means a
        // session with zero matching page views contributes a NULL,
        // which AVG ignores) — it is NOT "total page views / total
        // sessions including zero-page-view sessions". Documented here
        // and in `SessionStats`'s doc comment so this doesn't get
        // mis-read as the latter when Task F surfaces it.
        let row = sqlx::query_as::<_, SessionStatsRow>(
            r#"
            SELECT
                COUNT(*) AS session_count,
                AVG(pv_counts.view_count) AS avg_page_views_per_session,
                AVG(EXTRACT(EPOCH FROM (s.last_seen_at - s.started_at))) AS avg_session_duration_secs
            FROM rum_sessions s
            LEFT JOIN (
                SELECT session_id, COUNT(*) AS view_count
                FROM rum_page_views
                WHERE tenant_id = $1 AND site_id = $2 AND created_at >= $3 AND created_at < $4
                GROUP BY session_id
            ) pv_counts ON pv_counts.session_id = s.id
            WHERE s.tenant_id = $1 AND s.site_id = $2 AND s.started_at >= $3 AND s.started_at < $4
            "#,
        )
        .bind(tenant_id.0)
        .bind(site_id.0)
        .bind(range.start)
        .bind(range.end)
        .fetch_one(&self.pool)
        .await?;

        Ok(SessionStats {
            session_count: row.session_count,
            avg_page_views_per_session: row.avg_page_views_per_session,
            avg_session_duration_secs: row.avg_session_duration_secs,
        })
    }
}
