use std::sync::Arc;

use application::ports::{
    CheckRepository, IncidentRepository, MonitorRepository, NotificationChannelRepository,
    NotificationService,
};
use infra::UuidIdGenerator;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub monitor_repository: Arc<dyn MonitorRepository>,
    pub check_repository: Arc<dyn CheckRepository>,
    pub incident_repository: Arc<dyn IncidentRepository>,
    pub notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    pub notification_service: Arc<dyn NotificationService>,
    pub id_generator: UuidIdGenerator,
}

impl AppState {
    pub fn new<M, C, I, N, S>(
        monitor_repository: M,
        check_repository: C,
        incident_repository: I,
        notification_channel_repository: N,
        notification_service: S,
        id_generator: UuidIdGenerator,
    ) -> Self
    where
        M: MonitorRepository + 'static,
        C: CheckRepository + 'static,
        I: IncidentRepository + 'static,
        N: NotificationChannelRepository + 'static,
        S: NotificationService + 'static,
    {
        Self {
            monitor_repository: Arc::new(monitor_repository),
            check_repository: Arc::new(check_repository),
            incident_repository: Arc::new(incident_repository),
            notification_channel_repository: Arc::new(notification_channel_repository),
            notification_service: Arc::new(notification_service),
            id_generator,
        }
    }
}
