use application::ports::IdGenerator;
use async_trait::async_trait;
use domain::{CheckId, IncidentId, MonitorId, UserId};

#[derive(Debug, Default, Clone)]
pub struct UuidIdGenerator;

impl UuidIdGenerator {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl IdGenerator for UuidIdGenerator {
    async fn next_user_id(&self) -> UserId {
        UserId::generate()
    }

    async fn next_monitor_id(&self) -> MonitorId {
        MonitorId::generate()
    }

    async fn next_check_id(&self) -> CheckId {
        CheckId::generate()
    }

    async fn next_incident_id(&self) -> IncidentId {
        IncidentId::generate()
    }
}
