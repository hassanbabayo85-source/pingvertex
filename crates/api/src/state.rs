use std::sync::Arc;

use application::ports::{CheckRepository, IncidentRepository, MonitorRepository};
use infra::UuidIdGenerator;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub monitor_repository: Arc<dyn MonitorRepository>,
    pub check_repository: Arc<dyn CheckRepository>,
    pub incident_repository: Arc<dyn IncidentRepository>,
    pub id_generator: UuidIdGenerator,
}

impl AppState {
    pub fn new<M, C, I>(
        monitor_repository: M,
        check_repository: C,
        incident_repository: I,
        id_generator: UuidIdGenerator,
    ) -> Self
    where
        M: MonitorRepository + 'static,
        C: CheckRepository + 'static,
        I: IncidentRepository + 'static,
    {
        Self {
            monitor_repository: Arc::new(monitor_repository),
            check_repository: Arc::new(check_repository),
            incident_repository: Arc::new(incident_repository),
            id_generator,
        }
    }
}
