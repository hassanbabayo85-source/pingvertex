use std::error::Error;

use crate::{ports::MonitorRepository, queries::ListMonitors};
use domain::Monitor;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists all monitors through the application layer.
pub struct ListMonitorsUseCase<R> {
    repository: R,
}

impl<R> ListMonitorsUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, query: ListMonitors) -> UseCaseResult<Vec<Monitor>> {
        self.repository.find_all_for_user(query.user_id).await
    }
}
