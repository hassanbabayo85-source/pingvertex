use std::{error::Error, sync::Arc};

use crate::{
    dto::RecurringProxyMonitorResponse,
    ports::{EntitlementResolver, RecurringProxyMonitorRepository},
};
use domain::shared::types::{AuthenticatedUser, Permission};

type BoxError = Box<dyn Error + Send + Sync>;

pub struct ListRecurringProxyMonitorsUseCase<R> {
    repository: R,
    entitlement_resolver: Arc<dyn EntitlementResolver>,
}

impl<R> ListRecurringProxyMonitorsUseCase<R>
where
    R: RecurringProxyMonitorRepository,
{
    pub fn new(repository: R, entitlement_resolver: Arc<dyn EntitlementResolver>) -> Self {
        Self {
            repository,
            entitlement_resolver,
        }
    }

    pub async fn execute(
        &self,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Vec<RecurringProxyMonitorResponse>, BoxError> {
        authenticated_user.require(Permission::ViewMonitor)?;
        let tenant_id = authenticated_user.tenant_context().tenant_id();
        // Resolve entitlements so a downgraded tenant sees
        // `entitlement_suspended = true` on their existing monitors
        // (computed by the worker, not here).
        let _ = self.entitlement_resolver.resolve(tenant_id).await?;

        let monitors = self.repository.list_for_tenant(tenant_id).await?;
        Ok(monitors
            .iter()
            .map(RecurringProxyMonitorResponse::from_entity)
            .collect())
    }
}
