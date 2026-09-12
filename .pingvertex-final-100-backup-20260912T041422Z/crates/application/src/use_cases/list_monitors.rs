use std::error::Error;

use crate::{ports::MonitorRepository, queries::ListMonitors};
use domain::Monitor;
use domain::shared::types::AuthenticatedUser;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists all monitors through the application layer.
///
/// Tenant-scoped: returns every monitor belonging to the caller's tenant,
/// regardless of which member created each one.
pub struct ListMonitorsUseCase<R> {
    repository: R,
}

impl<R> ListMonitorsUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        _query: ListMonitors,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Vec<Monitor>> {
        self.repository
            .find_all_for_tenant(authenticated_user.tenant_context().tenant_id())
            .await
    }
}
