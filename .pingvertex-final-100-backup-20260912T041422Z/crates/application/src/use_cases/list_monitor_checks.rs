use std::{error::Error, fmt, sync::Arc};

use crate::ports::check_repository::CheckFilters;
use crate::ports::{CheckRepository, MonitorRepository};
use domain::Check;
use domain::shared::ids::MonitorId;
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::AuthenticatedUser;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

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
        limit: u32,
        offset: u32,
        filters: CheckFilters,
    ) -> Result<Vec<Check>, ListMonitorChecksError> {
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
            .list_by_monitor_paginated(monitor_id, limit, offset, filters)
            .await
            .map_err(ListMonitorChecksError::Other)
    }
}
