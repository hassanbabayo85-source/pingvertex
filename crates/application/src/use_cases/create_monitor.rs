use std::error::Error;

use crate::{
    dto::CreateMonitorRequest,
    ports::{IdGenerator, MonitorRepository},
};
use domain::{CheckInterval, ExpectedStatus, Monitor, MonitorBuilder, MonitorName, Timeout, Url};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Creates a monitor through the application layer.
pub struct CreateMonitorUseCase<R, G> {
    repository: R,
    id_generator: G,
}

impl<R, G> CreateMonitorUseCase<R, G>
where
    R: MonitorRepository,
    G: IdGenerator,
{
    pub fn new(repository: R, id_generator: G) -> Self {
        Self {
            repository,
            id_generator,
        }
    }

    pub async fn execute(&self, request: CreateMonitorRequest) -> UseCaseResult<Monitor> {
        let id = self.id_generator.next_monitor_id().await;

        let monitor = MonitorBuilder::new(id, request.user_id)
            .name(MonitorName::new(request.name)?)
            .url(Url::new(request.url)?)
            .method(request.method)
            .expected_status(ExpectedStatus::new(request.expected_status)?)
            .interval(CheckInterval::new(request.interval_seconds)?)
            .timeout(Timeout::new(request.timeout_seconds)?)
            .build();

        self.repository.save(&monitor).await?;

        Ok(monitor)
    }
}
