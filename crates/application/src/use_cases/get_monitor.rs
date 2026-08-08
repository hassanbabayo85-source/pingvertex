use std::error::Error;

use crate::{ports::MonitorRepository, queries::GetMonitor};
use domain::Monitor;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Gets a monitor through the application layer.
pub struct GetMonitorUseCase<R> {
    repository: R,
}

impl<R> GetMonitorUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, query: GetMonitor) -> UseCaseResult<Option<Monitor>> {
        self.repository
            .find_by_id_for_user(query.id, query.user_id)
            .await
    }
}
