use std::{error::Error, fmt, sync::Arc};

use crate::ports::{IncidentRepository, MonitorRepository};
use domain::Incident;
use domain::shared::ids::{MonitorId, UserId};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Error returned by `ListIncidentsUseCase`. Kept distinct from a generic
/// boxed error so the HTTP handler can tell a missing/foreign monitor
/// (→ 404) apart from a genuine backend failure (→ 500).
#[derive(Debug)]
pub enum ListIncidentsError {
    NotFound,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for ListIncidentsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "monitor not found or access denied"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for ListIncidentsError {}

/// Lists incidents for a monitor, verifying that the user owns the monitor.
#[derive(Clone)]
pub struct ListIncidentsUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    incident_repository: Arc<dyn IncidentRepository>,
}

impl ListIncidentsUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        incident_repository: Arc<dyn IncidentRepository>,
    ) -> Self {
        Self {
            monitor_repository,
            incident_repository,
        }
    }

    pub async fn execute(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> Result<Vec<Incident>, ListIncidentsError> {
        self.monitor_repository
            .find_by_id_for_user(monitor_id, user_id)
            .await
            .map_err(ListIncidentsError::Other)?
            .ok_or(ListIncidentsError::NotFound)?;

        self.incident_repository
            .list_by_monitor(monitor_id)
            .await
            .map_err(ListIncidentsError::Other)
    }
}
