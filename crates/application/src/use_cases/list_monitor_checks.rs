use std::{error::Error, sync::Arc};

use crate::ports::CheckRepository;
use domain::Check;
use domain::shared::ids::MonitorId;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists all checks for a monitor.
#[derive(Clone)]
pub struct ListMonitorChecksUseCase {
    repository: Arc<dyn CheckRepository>,
}

impl ListMonitorChecksUseCase {
    pub fn new(repository: Arc<dyn CheckRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, monitor_id: MonitorId) -> UseCaseResult<Vec<Check>> {
        self.repository.list_by_monitor(monitor_id).await
    }
}
