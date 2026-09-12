use std::error::Error;
use std::fmt;

use domain::errors::DomainError;
use domain::shared::ids::{RumJsErrorId, RumPageViewId, RumResourceErrorId, RumSessionId};
use domain::{
    RumJsError, RumPageView, RumResourceError, RumResourceType, RumSession, RumSite, RumVitals,
};

use crate::dto::RumIngestRequest;
use crate::ports::{RumEventRepository, RumSiteRepository};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Hard caps on how many error events a single beacon may carry.
///
/// Independent of the overall request body size limit (enforced at the
/// HTTP layer) — this specifically bounds how many INSERTs one ingestion
/// call can trigger, so a small-but-crafted payload (short messages, many
/// repetitions) can't be used to amplify write load disproportionately to
/// its byte size.
pub const MAX_JS_ERRORS_PER_BEACON: usize = 25;
pub const MAX_RESOURCE_ERRORS_PER_BEACON: usize = 25;

#[derive(Debug)]
pub enum IngestRumEventError {
    /// No `RumSite` matches the beacon's `site_key`. Deliberately the same
    /// external shape as any other malformed-payload rejection (see the
    /// handler) — this must not be usable as an oracle for enumerating
    /// valid site keys.
    UnknownSite,
    Validation(DomainError),
    TooManyJsErrors,
    TooManyResourceErrors,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for IngestRumEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSite => write!(f, "unknown RUM site"),
            Self::Validation(err) => write!(f, "{err}"),
            Self::TooManyJsErrors => write!(f, "too many JS errors in a single beacon"),
            Self::TooManyResourceErrors => {
                write!(f, "too many resource errors in a single beacon")
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for IngestRumEventError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

fn parse_device_type(value: Option<&str>) -> Option<domain::DeviceType> {
    match value {
        Some("desktop") => Some(domain::DeviceType::Desktop),
        Some("mobile") => Some(domain::DeviceType::Mobile),
        Some("tablet") => Some(domain::DeviceType::Tablet),
        // Anything unrecognized (including "other") is normalized to
        // `Other` rather than rejecting the whole beacon over a field
        // that's informational, not security-relevant.
        Some(_) => Some(domain::DeviceType::Other),
        None => None,
    }
}

fn parse_resource_type(value: Option<&str>) -> Option<RumResourceType> {
    match value {
        Some("script") => Some(RumResourceType::Script),
        Some("stylesheet") => Some(RumResourceType::Stylesheet),
        Some("image") => Some(RumResourceType::Image),
        Some("fetch") => Some(RumResourceType::Fetch),
        Some(_) => Some(RumResourceType::Other),
        None => None,
    }
}

/// Ingests one RUM beacon.
///
/// SECURITY (tenant isolation): `tenant_id` and `site_id` are ALWAYS
/// derived from the server-side `RumSiteRepository::find_by_site_key`
/// lookup below — nothing in `RumIngestRequest` is trusted as a tenant or
/// site identifier directly. This is the single choke point that makes
/// every write in this use case correctly tenant-scoped; do not add a
/// code path that persists a `RumSession`/`RumPageView`/error using any
/// tenant/site value other than the one returned by this lookup.
pub struct IngestRumEventUseCase<S, E> {
    site_repository: S,
    event_repository: E,
}

impl<S, E> IngestRumEventUseCase<S, E>
where
    S: RumSiteRepository,
    E: RumEventRepository,
{
    pub fn new(site_repository: S, event_repository: E) -> Self {
        Self {
            site_repository,
            event_repository,
        }
    }

    pub async fn execute(
        &self,
        request: RumIngestRequest,
        country_code: Option<String>,
    ) -> Result<(), IngestRumEventError> {
        self.validate_request(&request)?;

        let site = self
            .site_repository
            .find_by_site_key(&request.site_key)
            .await
            .map_err(IngestRumEventError::Other)?
            .ok_or(IngestRumEventError::UnknownSite)?;

        self.execute_for_site(request, country_code, site).await
    }

    /// Same ingestion path after the caller has already resolved the site.
    /// This avoids a second identical DB lookup on the public handler while
    /// retaining a server-side site-key equality check as the security
    /// boundary.
    pub async fn execute_for_site(
        &self,
        request: RumIngestRequest,
        country_code: Option<String>,
        site: RumSite,
    ) -> Result<(), IngestRumEventError> {
        self.validate_request(&request)?;
        if request.site_key != site.site_key {
            return Err(IngestRumEventError::UnknownSite);
        }

        let session = RumSession::new(
            RumSessionId::new(),
            site.tenant_id,
            site.id,
            request.session_key.clone(),
            request.browser_name.clone(),
            request.browser_version.clone(),
            request.os_name.clone(),
            parse_device_type(request.device_type.as_deref()),
            country_code,
        );

        let session = self
            .event_repository
            .upsert_session(&session)
            .await
            .map_err(IngestRumEventError::Other)?;

        let mut page_view_id = None;

        if let Some(pv) = request.page_view {
            let vitals = RumVitals {
                ttfb_ms: pv.ttfb_ms,
                fcp_ms: pv.fcp_ms,
                lcp_ms: pv.lcp_ms,
                inp_ms: pv.inp_ms,
                cls: pv.cls,
                load_time_ms: pv.load_time_ms,
            };

            let page_view = RumPageView::new(
                RumPageViewId::new(),
                site.tenant_id,
                site.id,
                session.id,
                pv.path,
                pv.referrer_path,
                vitals,
            )
            .map_err(IngestRumEventError::Validation)?;

            page_view_id = Some(page_view.id);

            self.event_repository
                .insert_page_view(&page_view)
                .await
                .map_err(IngestRumEventError::Other)?;
        }

        for js_error in request.js_errors {
            let error = RumJsError::new(
                RumJsErrorId::new(),
                site.tenant_id,
                site.id,
                session.id,
                page_view_id,
                js_error.message,
                js_error.source,
                js_error.line_number,
                js_error.column_number,
            )
            .map_err(IngestRumEventError::Validation)?;

            self.event_repository
                .insert_js_error(&error)
                .await
                .map_err(IngestRumEventError::Other)?;
        }

        for resource_error in request.resource_errors {
            let error = RumResourceError::new(
                RumResourceErrorId::new(),
                site.tenant_id,
                site.id,
                session.id,
                page_view_id,
                resource_error.resource_url,
                parse_resource_type(resource_error.resource_type.as_deref()),
                resource_error.status_code,
            )
            .map_err(IngestRumEventError::Validation)?;

            self.event_repository
                .insert_resource_error(&error)
                .await
                .map_err(IngestRumEventError::Other)?;
        }

        Ok(())
    }

    fn validate_request(&self, request: &RumIngestRequest) -> Result<(), IngestRumEventError> {
        if request.js_errors.len() > MAX_JS_ERRORS_PER_BEACON {
            return Err(IngestRumEventError::TooManyJsErrors);
        }
        if request.resource_errors.len() > MAX_RESOURCE_ERRORS_PER_BEACON {
            return Err(IngestRumEventError::TooManyResourceErrors);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use domain::RumSite;
    use domain::shared::ids::{RumSiteId, TenantId};

    use super::*;
    use crate::ports::rum_event_repository::RepoResult as EventRepoResult;
    use crate::ports::rum_site_repository::RepoResult as SiteRepoResult;

    #[derive(Default)]
    struct FakeSiteRepository {
        sites: Mutex<Vec<RumSite>>,
    }

    #[async_trait]
    impl RumSiteRepository for FakeSiteRepository {
        async fn create(&self, site: &RumSite) -> SiteRepoResult<()> {
            self.sites.lock().unwrap().push(site.clone());
            Ok(())
        }

        async fn find_by_site_key(&self, site_key: &str) -> SiteRepoResult<Option<RumSite>> {
            Ok(self
                .sites
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.site_key == site_key)
                .cloned())
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

    #[derive(Default)]
    struct FakeEventRepository {
        sessions: Mutex<Vec<RumSession>>,
        page_views: Mutex<Vec<RumPageView>>,
        js_errors: Mutex<Vec<RumJsError>>,
        resource_errors: Mutex<Vec<RumResourceError>>,
    }

    #[async_trait]
    impl RumEventRepository for FakeEventRepository {
        async fn upsert_session(&self, session: &RumSession) -> EventRepoResult<RumSession> {
            self.sessions.lock().unwrap().push(session.clone());
            Ok(session.clone())
        }

        async fn insert_page_view(&self, page_view: &RumPageView) -> EventRepoResult<()> {
            self.page_views.lock().unwrap().push(page_view.clone());
            Ok(())
        }

        async fn insert_js_error(&self, error: &RumJsError) -> EventRepoResult<()> {
            self.js_errors.lock().unwrap().push(error.clone());
            Ok(())
        }

        async fn insert_resource_error(&self, error: &RumResourceError) -> EventRepoResult<()> {
            self.resource_errors.lock().unwrap().push(error.clone());
            Ok(())
        }
    }

    fn sample_request(site_key: &str) -> RumIngestRequest {
        RumIngestRequest {
            site_key: site_key.to_string(),
            request_id: Some("rum-test-request-1".to_string()),
            session_key: "sess_123".to_string(),
            browser_name: Some("Chrome".to_string()),
            browser_version: Some("128".to_string()),
            os_name: Some("Windows".to_string()),
            device_type: Some("desktop".to_string()),
            page_view: Some(crate::dto::RumIngestPageView {
                path: "/pricing".to_string(),
                referrer_path: None,
                ttfb_ms: Some(100),
                fcp_ms: Some(500),
                lcp_ms: Some(900),
                inp_ms: Some(50),
                cls: Some(0.01),
                load_time_ms: Some(1200),
            }),
            js_errors: vec![],
            resource_errors: vec![],
        }
    }

    #[tokio::test]
    async fn pre_resolved_site_must_match_request_site_key() {
        let site_repo = FakeSiteRepository::default();
        let event_repo = FakeEventRepository::default();
        let tenant_id = TenantId::new();
        let site = RumSite::new(
            RumSiteId::new(),
            tenant_id,
            "My Site".to_string(),
            "example.com".to_string(),
        )
        .unwrap();
        site_repo.create(&site).await.unwrap();

        let use_case = IngestRumEventUseCase::new(site_repo, event_repo);
        let request = sample_request("rum_wrong_site_key");

        let err = use_case
            .execute_for_site(request, None, site)
            .await
            .unwrap_err();

        assert!(matches!(err, IngestRumEventError::UnknownSite));
    }

    #[tokio::test]
    async fn rejects_unknown_site_key() {
        let use_case = IngestRumEventUseCase::new(
            FakeSiteRepository::default(),
            FakeEventRepository::default(),
        );

        let err = use_case
            .execute(sample_request("rum_does_not_exist"), None)
            .await
            .unwrap_err();

        assert!(matches!(err, IngestRumEventError::UnknownSite));
    }

    #[tokio::test]
    async fn tenant_id_is_always_derived_from_the_resolved_site_never_from_the_request() {
        let site_repo = FakeSiteRepository::default();
        let event_repo = FakeEventRepository::default();

        let tenant_id = TenantId::new();
        let site = RumSite::new(
            RumSiteId::new(),
            tenant_id,
            "My Site".to_string(),
            "example.com".to_string(),
        )
        .unwrap();
        site_repo.create(&site).await.unwrap();

        let use_case = IngestRumEventUseCase::new(site_repo, event_repo);

        use_case
            .execute(sample_request(&site.site_key), None)
            .await
            .unwrap();

        let page_views = use_case.event_repository.page_views.lock().unwrap();
        assert_eq!(page_views.len(), 1);
        // The request DTO carries no tenant_id field at all — this
        // asserts the persisted row's tenant_id matches the SITE's tenant,
        // which is the only source it could have come from.
        assert_eq!(page_views[0].tenant_id, tenant_id);
    }

    #[tokio::test]
    async fn rejects_beacon_with_too_many_js_errors() {
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

        let use_case = IngestRumEventUseCase::new(site_repo, FakeEventRepository::default());

        let mut request = sample_request(&site.site_key);
        request.page_view = None;
        request.js_errors = (0..MAX_JS_ERRORS_PER_BEACON + 1)
            .map(|i| crate::dto::RumIngestJsError {
                message: format!("error {i}"),
                source: None,
                line_number: None,
                column_number: None,
            })
            .collect();

        let err = use_case.execute(request, None).await.unwrap_err();
        assert!(matches!(err, IngestRumEventError::TooManyJsErrors));
    }

    #[tokio::test]
    async fn propagates_domain_validation_errors() {
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

        let use_case = IngestRumEventUseCase::new(site_repo, FakeEventRepository::default());

        let mut request = sample_request(&site.site_key);
        // Query string on the path must be rejected by the domain layer.
        request.page_view.as_mut().unwrap().path = "/pricing?token=secret".to_string();

        let err = use_case.execute(request, None).await.unwrap_err();
        assert!(matches!(err, IngestRumEventError::Validation(_)));
    }
}
