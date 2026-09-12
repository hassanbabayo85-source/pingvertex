use std::error::Error;
use std::fmt;

use domain::shared::ids::{RumSiteId, TenantId};

use crate::dto::{
    BrowserDeviceBreakdown, GeoBreakdownRow, PagePerformanceRow, PerformanceTrendPoint,
    RumErrorBreakdown, RumOverview, RumTimeRange, SessionStats,
};
use crate::ports::{RumAnalyticsRepository, RumSiteRepository};

#[derive(Debug)]
pub enum RumDashboardError {
    /// No site with this ID belongs to the caller's tenant. Deliberately
    /// the same variant/response shape whether the site doesn't exist at
    /// all or exists but belongs to a different tenant — the caller
    /// can't distinguish the two, which is what makes this safe against
    /// site-ID enumeration across tenants.
    SiteNotFound,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for RumDashboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SiteNotFound => write!(f, "RUM site not found"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for RumDashboardError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

/// Every method here first resolves the requested `site_id` against the
/// CALLER'S tenant via `RumSiteRepository::find_by_id` (which itself
/// filters on both `id` AND `tenant_id` — see that port's doc comment)
/// before touching `RumAnalyticsRepository` at all. This is the single
/// choke point enforcing tenant isolation for every RUM dashboard
/// endpoint (Task F) — a handler that forgets to call through this use
/// case, and instead calls `RumAnalyticsRepository` directly with a
/// caller-supplied `site_id`, would reintroduce a cross-tenant data leak.
/// Don't add such a code path.
pub struct RumDashboardQueriesUseCase<S, A> {
    site_repository: S,
    analytics_repository: A,
}

impl<S, A> RumDashboardQueriesUseCase<S, A>
where
    S: RumSiteRepository,
    A: RumAnalyticsRepository,
{
    pub fn new(site_repository: S, analytics_repository: A) -> Self {
        Self {
            site_repository,
            analytics_repository,
        }
    }

    async fn resolve_site_id(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
    ) -> Result<RumSiteId, RumDashboardError> {
        let site = self
            .site_repository
            .find_by_id(site_id, tenant_id)
            .await
            .map_err(RumDashboardError::Other)?
            .ok_or(RumDashboardError::SiteNotFound)?;
        Ok(site.id)
    }

    pub async fn overview(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> Result<RumOverview, RumDashboardError> {
        let site_id = self.resolve_site_id(tenant_id, site_id).await?;
        self.analytics_repository
            .overview(tenant_id, site_id, range)
            .await
            .map_err(RumDashboardError::Other)
    }

    pub async fn slow_pages(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        limit: i64,
    ) -> Result<Vec<PagePerformanceRow>, RumDashboardError> {
        let site_id = self.resolve_site_id(tenant_id, site_id).await?;
        self.analytics_repository
            .slow_pages(tenant_id, site_id, range, limit)
            .await
            .map_err(RumDashboardError::Other)
    }

    pub async fn error_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        top_n: i64,
    ) -> Result<RumErrorBreakdown, RumDashboardError> {
        let site_id = self.resolve_site_id(tenant_id, site_id).await?;
        self.analytics_repository
            .error_breakdown(tenant_id, site_id, range, top_n)
            .await
            .map_err(RumDashboardError::Other)
    }

    pub async fn browser_device_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> Result<BrowserDeviceBreakdown, RumDashboardError> {
        let site_id = self.resolve_site_id(tenant_id, site_id).await?;
        self.analytics_repository
            .browser_device_breakdown(tenant_id, site_id, range)
            .await
            .map_err(RumDashboardError::Other)
    }

    pub async fn geo_breakdown(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
        limit: i64,
    ) -> Result<Vec<GeoBreakdownRow>, RumDashboardError> {
        let site_id = self.resolve_site_id(tenant_id, site_id).await?;
        self.analytics_repository
            .geo_breakdown(tenant_id, site_id, range, limit)
            .await
            .map_err(RumDashboardError::Other)
    }

    pub async fn performance_trend(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> Result<Vec<PerformanceTrendPoint>, RumDashboardError> {
        let site_id = self.resolve_site_id(tenant_id, site_id).await?;
        self.analytics_repository
            .performance_trend(tenant_id, site_id, range)
            .await
            .map_err(RumDashboardError::Other)
    }

    pub async fn session_stats(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        range: RumTimeRange,
    ) -> Result<SessionStats, RumDashboardError> {
        let site_id = self.resolve_site_id(tenant_id, site_id).await?;
        self.analytics_repository
            .session_stats(tenant_id, site_id, range)
            .await
            .map_err(RumDashboardError::Other)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use domain::RumSite;

    use super::*;
    use crate::dto::RumTimeRange;
    use crate::ports::rum_analytics_repository::RepoResult as AnalyticsRepoResult;
    use crate::ports::rum_site_repository::RepoResult as SiteRepoResult;

    #[derive(Default)]
    struct FakeSiteRepository {
        sites: std::sync::Mutex<Vec<RumSite>>,
    }

    #[async_trait]
    impl RumSiteRepository for FakeSiteRepository {
        async fn create(&self, site: &RumSite) -> SiteRepoResult<()> {
            self.sites.lock().unwrap().push(site.clone());
            Ok(())
        }
        async fn find_by_site_key(&self, _site_key: &str) -> SiteRepoResult<Option<RumSite>> {
            Ok(None)
        }
        async fn find_by_id(
            &self,
            id: RumSiteId,
            tenant_id: TenantId,
        ) -> SiteRepoResult<Option<RumSite>> {
            Ok(self
                .sites
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.id.0 == id.0 && s.tenant_id == tenant_id)
                .cloned())
        }
        async fn list_by_tenant(&self, tenant_id: TenantId) -> SiteRepoResult<Vec<RumSite>> {
            Ok(self
                .sites
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.tenant_id == tenant_id)
                .cloned()
                .collect())
        }
    }

    /// Records whether `overview` was ever actually called, so the
    /// tenant-isolation test can assert the analytics layer was never
    /// reached for a mismatched tenant, not just that the result was an
    /// error.
    #[derive(Default)]
    struct SpyAnalyticsRepository {
        overview_called: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl RumAnalyticsRepository for SpyAnalyticsRepository {
        async fn overview(
            &self,
            _tenant_id: TenantId,
            _site_id: RumSiteId,
            _range: RumTimeRange,
        ) -> AnalyticsRepoResult<RumOverview> {
            self.overview_called
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(RumOverview {
                page_view_count: 0,
                session_count: 0,
                avg_load_time_ms: None,
                p95_load_time_ms: None,
                p75_lcp_ms: None,
                p75_inp_ms: None,
                p75_cls: None,
                js_error_count: 0,
                resource_error_count: 0,
                error_rate: None,
            })
        }
        async fn slow_pages(
            &self,
            _: TenantId,
            _: RumSiteId,
            _: RumTimeRange,
            _: i64,
        ) -> AnalyticsRepoResult<Vec<PagePerformanceRow>> {
            Ok(vec![])
        }
        async fn error_breakdown(
            &self,
            _: TenantId,
            _: RumSiteId,
            _: RumTimeRange,
            _: i64,
        ) -> AnalyticsRepoResult<RumErrorBreakdown> {
            Ok(RumErrorBreakdown {
                js_error_count: 0,
                resource_error_count: 0,
                page_view_count: 0,
                error_rate: None,
                top_js_errors: vec![],
                top_resource_errors: vec![],
            })
        }
        async fn browser_device_breakdown(
            &self,
            _: TenantId,
            _: RumSiteId,
            _: RumTimeRange,
        ) -> AnalyticsRepoResult<BrowserDeviceBreakdown> {
            Ok(BrowserDeviceBreakdown {
                browsers: vec![],
                devices: vec![],
            })
        }
        async fn geo_breakdown(
            &self,
            _: TenantId,
            _: RumSiteId,
            _: RumTimeRange,
            _: i64,
        ) -> AnalyticsRepoResult<Vec<GeoBreakdownRow>> {
            Ok(vec![])
        }
        async fn performance_trend(
            &self,
            _: TenantId,
            _: RumSiteId,
            _: RumTimeRange,
        ) -> AnalyticsRepoResult<Vec<PerformanceTrendPoint>> {
            Ok(vec![])
        }
        async fn session_stats(
            &self,
            _: TenantId,
            _: RumSiteId,
            _: RumTimeRange,
        ) -> AnalyticsRepoResult<SessionStats> {
            Ok(SessionStats {
                session_count: 0,
                avg_page_views_per_session: None,
                avg_session_duration_secs: None,
            })
        }
    }

    fn sample_range() -> RumTimeRange {
        RumTimeRange {
            start: chrono::Utc::now() - chrono::Duration::days(1),
            end: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn rejects_site_from_a_different_tenant_without_ever_calling_analytics() {
        let site_repo = FakeSiteRepository::default();
        let owner_tenant = TenantId::new();
        let attacker_tenant = TenantId::new();

        let site = RumSite::new(
            RumSiteId::new(),
            owner_tenant,
            "Owner's Site".to_string(),
            "owner.example.com".to_string(),
        )
        .unwrap();
        site_repo.create(&site).await.unwrap();

        let analytics = Arc::new(SpyAnalyticsRepository::default());
        let use_case = RumDashboardQueriesUseCase::new(site_repo, analytics.clone());

        let err = use_case
            .overview(attacker_tenant, site.id, sample_range())
            .await
            .unwrap_err();

        assert!(matches!(err, RumDashboardError::SiteNotFound));
        assert!(
            !analytics
                .overview_called
                .load(std::sync::atomic::Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn allows_the_owning_tenant_and_reaches_analytics() {
        let site_repo = FakeSiteRepository::default();
        let tenant_id = TenantId::new();

        let site = RumSite::new(
            RumSiteId::new(),
            tenant_id,
            "My Site".to_string(),
            "example.com".to_string(),
        )
        .unwrap();
        site_repo.create(&site).await.unwrap();

        let analytics = Arc::new(SpyAnalyticsRepository::default());
        let use_case = RumDashboardQueriesUseCase::new(site_repo, analytics.clone());

        use_case
            .overview(tenant_id, site.id, sample_range())
            .await
            .unwrap();

        assert!(
            analytics
                .overview_called
                .load(std::sync::atomic::Ordering::SeqCst)
        );
    }
}
