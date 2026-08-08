use std::error::Error;

use crate::{commands::UpdateMonitor, ports::MonitorRepository};
use domain::{CheckInterval, ExpectedStatus, Monitor, MonitorName, Timeout, Url};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Updates an existing monitor through the application layer.
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
        // Tabbatar da cewa mutumin yana da ikon gyara wannan monitor din (User Ownership)
        let Some(mut monitor) = self
            .repository
            .find_by_id_for_user(command.id, command.user_id)
            .await?
        else {
            return Ok(None);
        };

        monitor.name = MonitorName::new(command.name)?;
        monitor.url = Url::new(command.url)?.as_str().to_owned();
        monitor.method = command.method;
        monitor.expected_status = ExpectedStatus::new(command.expected_status)?.into();
        monitor.interval_seconds = CheckInterval::new(command.interval_seconds)?.into();
        monitor.timeout_seconds = Timeout::new(command.timeout_seconds)?.into();

        // Muna amfani da update() domin Mun riga mun tabbatar da Ownership a sama
        self.repository.update(&monitor).await?;

        Ok(Some(monitor))
    }
}
