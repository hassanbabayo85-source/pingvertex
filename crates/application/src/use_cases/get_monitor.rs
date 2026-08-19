use std::error::Error;

use crate::{ports::MonitorRepository, queries::GetMonitor};
use domain::Monitor;
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::AuthenticatedUser;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Gets a monitor through the application layer.
///
/// Tenant-scoped: any authenticated member of the monitor's tenant can
/// fetch it, not just the member who originally created it.
pub struct GetMonitorUseCase<R> {
    repository: R,
}

impl<R> GetMonitorUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        query: GetMonitor,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<Monitor>> {
        let monitor = match self.repository.find_by_id(query.id).await? {
            Some(monitor) => monitor,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Ok(None);
        }

        Ok(Some(monitor))
    }
}
