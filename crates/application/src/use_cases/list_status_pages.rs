use std::error::Error;

use crate::pagination::{Page, PageRequest};
use crate::ports::StatusPageRepository;
use domain::shared::ids::TenantId;
use domain::{entities::StatusPage, shared::types::AuthenticatedUser};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct ListStatusPagesUseCase<R> {
    repository: R,
}

impl<R> ListStatusPagesUseCase<R>
where
    R: StatusPageRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Vec<StatusPage>> {
        self.execute_for_tenant(authenticated_user.tenant_context().tenant_id())
            .await
    }

    /// Tenant-scoped variant of `execute` that takes a `TenantId`
    /// directly instead of an `AuthenticatedUser`.
    pub async fn execute_for_tenant(&self, tenant_id: TenantId) -> UseCaseResult<Vec<StatusPage>> {
        self.repository.find_all_for_tenant(tenant_id).await
    }

    /// Cursor-paginated entry point for `GET /api/v1/status-pages`.
    pub async fn execute_paginated(
        &self,
        authenticated_user: AuthenticatedUser,
        page: PageRequest,
    ) -> UseCaseResult<Page<StatusPage>> {
        self.execute_paginated_for_tenant(authenticated_user.tenant_context().tenant_id(), page)
            .await
    }

    /// Tenant-scoped cursor-paginated variant.
    /// Used by external api_key based endpoint.
    pub async fn execute_paginated_for_tenant(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> UseCaseResult<Page<StatusPage>> {
        self.repository
            .find_all_for_tenant_paginated(tenant_id, page)
            .await
    }
}
