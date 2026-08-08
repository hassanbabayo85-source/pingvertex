use async_trait::async_trait;
use domain::Monitor;
use std::error::Error;

#[async_trait]
pub trait NotificationService: Send + Sync {
    async fn send_monitor_down(
        &self,
        monitor: &Monitor,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    async fn send_monitor_recovered(
        &self,
        monitor: &Monitor,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
}
