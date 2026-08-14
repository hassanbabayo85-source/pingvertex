use std::error::Error;

use crate::{
    commands::DeleteApiFlow,
    ports::{ApiFlowRepository, MonitorRepository},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct DeleteApiFlowUseCase<R, M> {
    api_flow_repository: R,
    monitor_repository: M,
}

impl<R, M> DeleteApiFlowUseCase<R, M>
where
    R: ApiFlowRepository,
    M: MonitorRepository,
{
    pub fn new(api_flow_repository: R, monitor_repository: M) -> Self {
        Self {
            api_flow_repository,
            monitor_repository,
        }
    }

    pub async fn execute(&self, command: DeleteApiFlow) -> UseCaseResult<bool> {
        if self
            .monitor_repository
            .find_by_id_for_user(command.monitor_id, command.user_id)
            .await?
            .is_none()
        {
            return Ok(false);
        }

        let flow = match self.api_flow_repository.find_by_id(command.id).await? {
            Some(flow) => flow,
            None => return Ok(false),
        };

        if flow.monitor_id != command.monitor_id {
            return Ok(false);
        }

        self.api_flow_repository.delete(command.id).await
    }
}
