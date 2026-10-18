use std::{error::Error, fmt, sync::Arc};

use crate::{
    dto::{RecurringProxyMonitorResponse, UpdateRecurringProxyMonitorRequest},
    errors::PlanLimitError,
    ports::{EntitlementResolver, RecurringProxyMonitorRepository},
};
use domain::entities::MIN_RECURRING_PROXY_INTERVAL_SECONDS;
use domain::shared::types::{AuthenticatedUser, Permission};
use uuid::Uuid;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
pub enum UpdateRecurringProxyMonitorError {
    Forbidden,
    NotFound,
    PlanLimitExceeded(PlanLimitError),
    Other(BoxError),
}

impl fmt::Display for UpdateRecurringProxyMonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(f, "forbidden"),
            Self::NotFound => write!(f, "recurring proxy monitor not found"),
            Self::PlanLimitExceeded(e) => write!(f, "{e}"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for UpdateRecurringProxyMonitorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlanLimitExceeded(e) => Some(e),
            Self::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct UpdateRecurringProxyMonitorUseCase<R> {
    repository: R,
    entitlement_resolver: Arc<dyn EntitlementResolver>,
}

impl<R> UpdateRecurringProxyMonitorUseCase<R>
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
        id: Uuid,
        request: UpdateRecurringProxyMonitorRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<RecurringProxyMonitorResponse, UpdateRecurringProxyMonitorError> {
        if !authenticated_user.can(Permission::UpdateMonitor) {
            return Err(UpdateRecurringProxyMonitorError::Forbidden);
        }
        let tenant_id = authenticated_user.tenant_context().tenant_id();
        let entitlements = self
            .entitlement_resolver
            .resolve(tenant_id)
            .await
            .map_err(UpdateRecurringProxyMonitorError::Other)?;

        if request.interval_seconds < entitlements.min_recurring_proxy_interval_seconds {
            return Err(UpdateRecurringProxyMonitorError::PlanLimitExceeded(
                PlanLimitError::new(
                    MIN_RECURRING_PROXY_INTERVAL_SECONDS,
                    u64::from(request.interval_seconds),
                    u64::from(entitlements.min_recurring_proxy_interval_seconds),
                    "upgrade_plan",
                ),
            ));
        }

        let mut monitor = self
            .repository
            .find_by_id(id, tenant_id)
            .await
            .map_err(UpdateRecurringProxyMonitorError::Other)?
            .ok_or(UpdateRecurringProxyMonitorError::NotFound)?;

        monitor.name = request.name;
        monitor.set_interval(request.interval_seconds).map_err(|e| {
            UpdateRecurringProxyMonitorError::Other(Box::new(e))
        })?;

        self.repository
            .update_monitor(&monitor)
            .await
            .map_err(UpdateRecurringProxyMonitorError::Other)?;

        Ok(RecurringProxyMonitorResponse::from_entity(&monitor))
    }
}
