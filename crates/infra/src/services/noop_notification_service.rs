use application::ports::NotificationService;
use async_trait::async_trait;
use domain::Monitor;
use std::error::Error;

#[derive(Debug, Clone, Default)]
pub struct NoopNotificationService;

impl NoopNotificationService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl NotificationService for NoopNotificationService {
    async fn send_monitor_down(
        &self,
        _monitor: &Monitor,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    async fn send_monitor_recovered(
        &self,
        _monitor: &Monitor,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}
