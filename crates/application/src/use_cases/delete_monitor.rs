use std::error::Error;

use crate::{commands::DeleteMonitor, ports::MonitorRepository};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Deletes an existing monitor through the application layer.
pub struct DeleteMonitorUseCase<R> {
    repository: R,
}

impl<R> DeleteMonitorUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, command: DeleteMonitor) -> UseCaseResult<bool> {
        self.repository
            .delete_for_user(command.id, command.user_id)
            .await
    }
}
