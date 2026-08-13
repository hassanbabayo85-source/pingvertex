use std::error::Error;

use crate::{commands::UpdateMonitor, ports::MonitorRepository};

use domain::{
    CheckInterval, ExpectedStatus, Monitor, MonitorName, Timeout, Url, entities::Assertion,
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct UpdateMonitorUseCase<R> {
    repository: R,
}

impl<R> UpdateMonitorUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, command: UpdateMonitor) -> UseCaseResult<Option<Monitor>> {
        let mut monitor = match self
            .repository
            .find_by_id_for_user(command.id, command.user_id)
            .await?
        {
            Some(monitor) => monitor,
            None => return Ok(None),
        };

        monitor.update(
            MonitorName::new(command.name)?,
            Url::new(command.url)?,
            command.method,
            ExpectedStatus::new(command.expected_status)?,
            CheckInterval::new(command.interval_seconds)?,
            Timeout::new(command.timeout_seconds)?,
        );

        monitor.is_public = command.is_public;
        monitor.ssl_check_enabled = command.ssl_check_enabled;
        monitor.ssl_expiry_warning_days = command.ssl_expiry_warning_days;

        monitor.assertions.clear();

        for input in &command.assertions {
            let assertion = Assertion::new(
                input.response_part.clone(),
                input.operator.clone(),
                input.target.clone(),
                input.expected_value.clone(),
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

            monitor
                .add_assertion(assertion)
                .map_err(Box::<dyn Error + Send + Sync>::from)?;
        }

        self.repository.save(&monitor).await?;

        Ok(Some(monitor))
    }
}
