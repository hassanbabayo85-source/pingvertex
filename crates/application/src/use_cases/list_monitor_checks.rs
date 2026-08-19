use std::{error::Error, fmt, sync::Arc};

use crate::ports::{CheckRepository, MonitorRepository};
use domain::Check;
use domain::shared::ids::MonitorId;
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::AuthenticatedUser;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Error returned by `ListMonitorChecksUseCase`. Kept distinct from a
/// generic boxed error so the HTTP handler can tell a missing/foreign
/// monitor (→ 404) apart from a genuine backend failure (→ 500).
#[derive(Debug)]
pub enum ListMonitorChecksError {
    NotFound,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for ListMonitorChecksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "monitor not found or access denied"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for ListMonitorChecksError {}

/// Lists checks for a monitor, verifying that the user owns the monitor.
#[derive(Clone)]
pub struct ListMonitorChecksUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    check_repository: Arc<dyn CheckRepository>,
}

impl ListMonitorChecksUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        check_repository: Arc<dyn CheckRepository>,
    ) -> Self {
        Self {
            monitor_repository,
            check_repository,
        }
    }

    pub async fn execute(
        &self,
        monitor_id: MonitorId,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Vec<Check>, ListMonitorChecksError> {
        // Tabbatar da cewa monitor ɗin na tenant ɗin user ne
        let monitor = self
            .monitor_repository
            .find_by_id(monitor_id)
            .await
            .map_err(ListMonitorChecksError::Other)?
            .ok_or(ListMonitorChecksError::NotFound)?;

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Err(ListMonitorChecksError::NotFound);
        }

        self.check_repository
            .list_by_monitor(monitor_id)
            .await
            .map_err(ListMonitorChecksError::Other)
    }
}
