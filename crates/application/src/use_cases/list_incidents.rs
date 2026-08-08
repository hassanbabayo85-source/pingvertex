use std::{error::Error, sync::Arc};

use crate::ports::IncidentRepository;
use domain::Incident;
use domain::shared::ids::MonitorId;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists all incidents for a monitor.
#[derive(Clone)]
pub struct ListIncidentsUseCase {
    repository: Arc<dyn IncidentRepository>,
}

impl ListIncidentsUseCase {
    pub fn new(repository: Arc<dyn IncidentRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, monitor_id: MonitorId) -> UseCaseResult<Vec<Incident>> {
        self.repository.list_by_monitor(monitor_id).await
    }
}
