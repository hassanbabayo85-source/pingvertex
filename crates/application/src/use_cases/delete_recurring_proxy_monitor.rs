use std::{error::Error, fmt};

use crate::ports::RecurringProxyMonitorRepository;
use domain::shared::types::{AuthenticatedUser, Permission};
use uuid::Uuid;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
pub enum DeleteRecurringProxyMonitorError {
    Forbidden,
    NotFound,
    Other(BoxError),
}

impl fmt::Display for DeleteRecurringProxyMonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(f, "forbidden"),
            Self::NotFound => write!(f, "recurring proxy monitor not found"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for DeleteRecurringProxyMonitorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct DeleteRecurringProxyMonitorUseCase<R> {
    repository: R,
}

impl<R: RecurringProxyMonitorRepository> DeleteRecurringProxyMonitorUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        id: Uuid,
        authenticated_user: AuthenticatedUser,
    ) -> Result<(), DeleteRecurringProxyMonitorError> {
        if !authenticated_user.can(Permission::DeleteMonitor) {
            return Err(DeleteRecurringProxyMonitorError::Forbidden);
        }
        let tenant_id = authenticated_user.tenant_context().tenant_id();
        let removed = self
            .repository
            .delete_for_tenant(id, tenant_id)
            .await
            .map_err(DeleteRecurringProxyMonitorError::Other)?;
        if !removed {
            return Err(DeleteRecurringProxyMonitorError::NotFound);
        }
        Ok(())
    }
}
