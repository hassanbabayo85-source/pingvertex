use std::error::Error;

use async_trait::async_trait;
use domain::shared::ids::{RumSiteId, TenantId};

use crate::dto::{
    BrowserDeviceBreakdown, GeoBreakdownRow, PagePerformanceRow, PerformanceTrendPoint,
    RumErrorBreakdown, RumOverview, RumTimeRange, SessionStats,
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Read-only aggregation queries backing the RUM dashboard (Task F).
///
/// Every method takes `tenant_id` AND `site_id` and is expected to filter
/// on both (defense in depth, matching the same pattern used everywhere
/// else in this schema — see `TenantOwned`/Task B's migration notes),
/// even though `site_id` alone would already scope the data correctly
/// given `RumSiteRepository::find_by_id`'s tenant check happens before
/// any of these are called (Task F's job, not this port's).
///
/// Implementations MUST do this in a single aggregate SQL query per
/// method (no per-row loop / no N+1) — see
/// `infra::PostgresRumAnalyticsRepository` and the composite indexes
/// added in `migrations/0064_...`.
#[async_trait]
pub trait RumAnalyticsRepository: Send + Sync {
    async fn overview(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<RumOverview>;

    /// Per-path performance breakdown, ordered slowest-first by
    /// `avg_load_time_ms`. Covers both "page performance" and
    /// "slow pages" from the brief — they're the same query sorted the
    /// same way.
    async fn slow_pages(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        limit: i64,
    ) -> RepoResult<Vec<PagePerformanceRow>>;

    async fn error_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        top_n: i64,
    ) -> RepoResult<RumErrorBreakdown>;

    async fn browser_device_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<BrowserDeviceBreakdown>;

    /// Empty (or all-`None`-country) results are expected and NOT an
    /// error until geo-IP lookup is configured — see `GeoBreakdownRow`'s
    /// doc comment.
    async fn geo_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        limit: i64,
    ) -> RepoResult<Vec<GeoBreakdownRow>>;

    /// Fixed hourly buckets — see `PerformanceTrendPoint`'s doc comment.
    async fn performance_trend(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<Vec<PerformanceTrendPoint>>;

    async fn session_stats(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<SessionStats>;
}

#[async_trait]
impl<T> RumAnalyticsRepository for std::sync::Arc<T>
where
    T: RumAnalyticsRepository + ?Sized,
{
    async fn overview(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<RumOverview> {
        (**self).overview(tenant_id, site_id, range).await
    }

    async fn slow_pages(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        limit: i64,
    ) -> RepoResult<Vec<PagePerformanceRow>> {
        (**self).slow_pages(tenant_id, site_id, range, limit).await
    }

    async fn error_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        top_n: i64,
    ) -> RepoResult<RumErrorBreakdown> {
        (**self)
            .error_breakdown(tenant_id, site_id, range, top_n)
            .await
    }

    async fn browser_device_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<BrowserDeviceBreakdown> {
        (**self)
            .browser_device_breakdown(tenant_id, site_id, range)
            .await
    }

    async fn geo_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        limit: i64,
    ) -> RepoResult<Vec<GeoBreakdownRow>> {
        (**self)
            .geo_breakdown(tenant_id, site_id, range, limit)
            .await
    }

    async fn performance_trend(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<Vec<PerformanceTrendPoint>> {
        (**self).performance_trend(tenant_id, site_id, range).await
    }

    async fn session_stats(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> RepoResult<SessionStats> {
        (**self).session_stats(tenant_id, site_id, range).await
    }
}
