use std::error::Error;

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
    ///
    /// Added for the API-key-authenticated public surface
    /// (`GET /api/v1/external/status-pages`, B31) — see the doc comment
    /// on `ListChecksUseCase::execute_for_tenant` for why a separate
    /// method exists instead of synthesizing an `AuthenticatedUser` from
    /// an `ApiKeyPrincipal`.
    pub async fn execute_for_tenant(&self, tenant_id: TenantId) -> UseCaseResult<Vec<StatusPage>> {
        self.repository.find_all_for_tenant(tenant_id).await
    }
}
