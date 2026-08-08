use async_trait::async_trait;
use domain::{CheckId, IncidentId, MonitorId, UserId};

/// Generates unique IDs for domain entities.
#[async_trait]
pub trait IdGenerator: Send + Sync {
    async fn next_user_id(&self) -> UserId;

    async fn next_monitor_id(&self) -> MonitorId;

    async fn next_check_id(&self) -> CheckId;

    async fn next_incident_id(&self) -> IncidentId;
}
