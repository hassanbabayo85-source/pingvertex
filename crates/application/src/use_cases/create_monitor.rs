use std::{error::Error, fmt};

use crate::{
    dto::CreateMonitorRequest,
    ports::{IdGenerator, MonitorRepository},
};
use domain::{
    CheckInterval, ExpectedStatus, Monitor, MonitorBuilder, MonitorName, Timeout, Url,
    entities::Assertion,
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Hard cap on the number of monitors a single user may create on the free plan.
///
/// Without this limit a single account could create thousands of monitors that are
/// polled every 30 seconds forever, causing unbounded worker and database costs.
pub const MAX_MONITORS_PER_USER: usize = 10;

#[derive(Debug)]
pub enum CreateMonitorError {
    QuotaExceeded { limit: usize },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateMonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QuotaExceeded { limit } => {
                write!(
                    f,
                    "monitor limit reached: free plan allows at most {limit} monitors"
                )
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateMonitorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::QuotaExceeded { .. } => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

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

    pub async fn execute(
        &self,
        request: CreateMonitorRequest,
    ) -> Result<Monitor, CreateMonitorError> {
        let monitors = self
            .repository
            .find_all_for_user(request.user_id)
            .await
            .map_err(|e| CreateMonitorError::Other(e.into()))?;

        if monitors.len() >= MAX_MONITORS_PER_USER {
            return Err(CreateMonitorError::QuotaExceeded {
                limit: MAX_MONITORS_PER_USER,
            });
        }

        self.build_and_save(request)
            .await
            .map_err(CreateMonitorError::Other)
    }

    async fn build_and_save(&self, request: CreateMonitorRequest) -> UseCaseResult<Monitor> {
        let id = self.id_generator.next_monitor_id().await;

        let mut monitor = MonitorBuilder::new(id, request.user_id)
            .name(MonitorName::new(request.name)?)
            .url(Url::new(request.url)?)
            .method(request.method)
            .expected_status(ExpectedStatus::new(request.expected_status)?)
            .interval(CheckInterval::new(request.interval_seconds)?)
            .timeout(Timeout::new(request.timeout_seconds)?)
            .build();

        monitor.is_public = request.is_public;
        monitor.ssl_check_enabled = request.ssl_check_enabled;
        monitor.ssl_expiry_warning_days = request.ssl_expiry_warning_days;

        // Add assertions from the request
        for input in &request.assertions {
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

        Ok(monitor)
    }
}
