use std::{error::Error, fmt};

use crate::{dto::RecurringProxyCheckResponse, ports::RecurringProxyMonitorRepository};
use domain::shared::types::{AuthenticatedUser, Permission};
use uuid::Uuid;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
pub enum GetLatestRecurringProxyCheckError {
    Forbidden,
    NotFound,
    Other(BoxError),
}

impl fmt::Display for GetLatestRecurringProxyCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(f, "forbidden"),
            Self::NotFound => write!(f, "recurring proxy monitor not found"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for GetLatestRecurringProxyCheckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct GetLatestRecurringProxyCheckUseCase<R> {
    repository: R,
}

impl<R: RecurringProxyMonitorRepository> GetLatestRecurringProxyCheckUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        monitor_id: Uuid,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Option<RecurringProxyCheckResponse>, GetLatestRecurringProxyCheckError> {
        if !authenticated_user.can(Permission::ViewMonitor) {
            return Err(GetLatestRecurringProxyCheckError::Forbidden);
        }
        let tenant_id = authenticated_user.tenant_context().tenant_id();
        let exists = self
            .repository
            .find_by_id(monitor_id, tenant_id)
            .await
            .map_err(GetLatestRecurringProxyCheckError::Other)?
            .is_some();
        if !exists {
            return Err(GetLatestRecurringProxyCheckError::NotFound);
        }
        let latest = self
            .repository
            .find_latest_check(monitor_id, tenant_id)
            .await
            .map_err(GetLatestRecurringProxyCheckError::Other)?;
        Ok(latest.map(|c| RecurringProxyCheckResponse::from_entity(&c)))
    }
}
