use std::{error::Error, fmt};

use crate::{dto::RecurringProxyCheckResponse, ports::RecurringProxyMonitorRepository};
use domain::shared::types::{AuthenticatedUser, Permission};
use uuid::Uuid;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
pub enum ListRecurringProxyChecksError {
    Forbidden,
    NotFound,
    Other(BoxError),
}

impl fmt::Display for ListRecurringProxyChecksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(f, "forbidden"),
            Self::NotFound => write!(f, "recurring proxy monitor not found"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for ListRecurringProxyChecksError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct ListRecurringProxyChecksResult {
    pub checks: Vec<RecurringProxyCheckResponse>,
    pub total: u64,
}

pub struct ListRecurringProxyChecksUseCase<R> {
    repository: R,
}

impl<R: RecurringProxyMonitorRepository> ListRecurringProxyChecksUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        monitor_id: Uuid,
        offset: u32,
        limit: u32,
        authenticated_user: AuthenticatedUser,
    ) -> Result<ListRecurringProxyChecksResult, ListRecurringProxyChecksError> {
        if !authenticated_user.can(Permission::ViewMonitor) {
            return Err(ListRecurringProxyChecksError::Forbidden);
        }
        let tenant_id = authenticated_user.tenant_context().tenant_id();
        // Verify the monitor belongs to this tenant first.
        let exists = self
            .repository
            .find_by_id(monitor_id, tenant_id)
            .await
            .map_err(ListRecurringProxyChecksError::Other)?
            .is_some();
        if !exists {
            return Err(ListRecurringProxyChecksError::NotFound);
        }

        let (checks, total) = self
            .repository
            .list_checks(monitor_id, tenant_id, offset, limit)
            .await
            .map_err(ListRecurringProxyChecksError::Other)?;

        Ok(ListRecurringProxyChecksResult {
            checks: checks
                .iter()
                .map(RecurringProxyCheckResponse::from_entity)
                .collect(),
            total,
        })
    }
}
