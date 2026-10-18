use std::error::Error;
use std::fmt;

use domain::RumSite;
use domain::errors::DomainError;
use domain::shared::ids::TenantId;

use crate::pagination::{Page, PageRequest};
use crate::ports::RumSiteRepository;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
pub enum CreateRumSiteError {
    Validation(DomainError),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateRumSiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(err) => write!(f, "{err}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateRumSiteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

/// Creates a new RUM site (a domain/property a tenant wants RUM data
/// for), generating its public `site_key` server-side.
///
/// This is the "Task F gap" flagged back in Task C's HANDOVER notes:
/// without this, there was no way to actually get a `site_key` to hand a
/// customer for the browser SDK (Task D) to use.
pub struct CreateRumSiteUseCase<S> {
    site_repository: S,
}

impl<S> CreateRumSiteUseCase<S>
where
    S: RumSiteRepository,
{
    pub fn new(site_repository: S) -> Self {
        Self { site_repository }
    }

    pub async fn execute(
        &self,
        tenant_id: TenantId,
        name: String,
        domain: String,
    ) -> Result<RumSite, CreateRumSiteError> {
        let site = RumSite::new(
            domain::shared::ids::RumSiteId::new(),
            tenant_id,
            name,
            domain,
        )
        .map_err(CreateRumSiteError::Validation)?;

        self.site_repository
            .create(&site)
            .await
            .map_err(CreateRumSiteError::Other)?;

        Ok(site)
    }
}

/// Lists all RUM sites belonging to a tenant.
pub struct ListRumSitesUseCase<S> {
    site_repository: S,
}

impl<S> ListRumSitesUseCase<S>
where
    S: RumSiteRepository,
{
    pub fn new(site_repository: S) -> Self {
        Self { site_repository }
    }

    pub async fn execute(&self, tenant_id: TenantId) -> UseCaseResult<Vec<RumSite>> {
        self.site_repository.list_by_tenant(tenant_id).await
    }

    /// PART 8 — cursor-paginated entry point for
    /// `GET /api/v1/dashboard/rum/sites`.
    pub async fn execute_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> UseCaseResult<Page<RumSite>> {
        self.site_repository
            .list_by_tenant_paginated(tenant_id, page)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use domain::shared::ids::RumSiteId;

    use super::*;
    use crate::ports::rum_site_repository::RepoResult;

    #[derive(Default)]
    struct FakeSiteRepository {
        sites: Mutex<Vec<RumSite>>,
    }

    #[async_trait]
    impl RumSiteRepository for FakeSiteRepository {
        async fn create(&self, site: &RumSite) -> RepoResult<()> {
            self.sites.lock().unwrap().push(site.clone());
            Ok(())
        }

        async fn find_by_site_key(&self, site_key: &str) -> RepoResult<Option<RumSite>> {
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
        ) -> RepoResult<Option<RumSite>> {
            Ok(self
                .sites
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.id.0 == id.0 && s.tenant_id == tenant_id)
                .cloned())
        }

        async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<RumSite>> {
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

    #[tokio::test]
    async fn create_rejects_empty_name() {
        let use_case = CreateRumSiteUseCase::new(FakeSiteRepository::default());
        let err = use_case
            .execute(TenantId::new(), "".to_string(), "example.com".to_string())
            .await
            .unwrap_err();
        assert!(matches!(err, CreateRumSiteError::Validation(_)));
    }

    #[tokio::test]
    async fn list_only_returns_sites_for_the_given_tenant() {
        let repo = std::sync::Arc::new(FakeSiteRepository::default());
        let tenant_a = TenantId::new();
        let tenant_b = TenantId::new();

        let create = CreateRumSiteUseCase::new(repo.clone());
        create
            .execute(
                tenant_a,
                "A's site".to_string(),
                "a.example.com".to_string(),
            )
            .await
            .unwrap();
        create
            .execute(
                tenant_b,
                "B's site".to_string(),
                "b.example.com".to_string(),
            )
            .await
            .unwrap();

        let list = ListRumSitesUseCase::new(repo);
        let sites = list.execute(tenant_a).await.unwrap();

        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].tenant_id, tenant_a);
    }
}
