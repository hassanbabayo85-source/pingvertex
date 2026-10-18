use std::{error::Error, fmt};

use crate::{
    dto::RecurringProxyMonitorResponse,
    ports::RecurringProxyMonitorRepository,
};
use domain::shared::types::{AuthenticatedUser, Permission};
use uuid::Uuid;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
pub enum SetEnabledError {
    Forbidden,
    NotFound,
    Other(BoxError),
}

impl fmt::Display for SetEnabledError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(f, "forbidden"),
            Self::NotFound => write!(f, "recurring proxy monitor not found"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for SetEnabledError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct SetEnabledUseCase<R> {
    repository: R,
}

impl<R: RecurringProxyMonitorRepository> SetEnabledUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        id: Uuid,
        enabled: bool,
        authenticated_user: AuthenticatedUser,
    ) -> Result<RecurringProxyMonitorResponse, SetEnabledError> {
        if !authenticated_user.can(Permission::UpdateMonitor) {
            return Err(SetEnabledError::Forbidden);
        }
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut monitor = self
            .repository
            .find_by_id(id, tenant_id)
            .await
            .map_err(SetEnabledError::Other)?
            .ok_or(SetEnabledError::NotFound)?;

        monitor.set_user_enabled(enabled);
        self.repository
            .update_monitor(&monitor)
            .await
            .map_err(SetEnabledError::Other)?;

        Ok(RecurringProxyMonitorResponse::from_entity(&monitor))
    }
}
