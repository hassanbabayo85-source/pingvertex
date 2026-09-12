use std::sync::Arc;

use crate::{
    entities::Monitor,
    repositories::MonitorRepository,
};

pub struct CreateMonitorUseCase<R>
where
    R: MonitorRepository,
{
    repository: Arc<R>,
}

impl<R> CreateMonitorUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, monitor: Monitor) {
        self.repository.create(&monitor).await;
    }
}
