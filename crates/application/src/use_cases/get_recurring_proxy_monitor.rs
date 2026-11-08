use std::{error::Error, sync::Arc};

use crate::{
    dto::RecurringProxyMonitorResponse,
    ports::{RecurringProxyMonitorRepository, ResourceLimitResolver},
};
use domain::shared::types::{AuthenticatedUser, Permission};
use uuid::Uuid;

type BoxError = Box<dyn Error + Send + Sync>;

pub struct GetRecurringProxyMonitorUseCase<R> {
    repository: R,
    resource_limit_resolver: Arc<dyn ResourceLimitResolver>,
}

impl<R> GetRecurringProxyMonitorUseCase<R>
where
    R: RecurringProxyMonitorRepository,
{
    pub fn new(repository: R, resource_limit_resolver: Arc<dyn ResourceLimitResolver>) -> Self {
        Self {
            repository,
            resource_limit_resolver,
        }
    }

    pub async fn execute(
        &self,
        id: Uuid,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Option<RecurringProxyMonitorResponse>, BoxError> {
        authenticated_user.require(Permission::ViewMonitor)?;
        let tenant_id = authenticated_user.tenant_context().tenant_id();
        let _ = self.resource_limit_resolver.resolve(tenant_id).await?;

        let monitor = self.repository.find_by_id(id, tenant_id).await?;
        Ok(monitor.map(|m| RecurringProxyMonitorResponse::from_entity(&m)))
    }
}
