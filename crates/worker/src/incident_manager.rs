use std::{error::Error, sync::Arc};

use application::ports::{
    IdGenerator, IncidentRepository, MaintenanceWindowRepository, NotificationChannelRepository,
    NotificationService,
};
use async_trait::async_trait;
use domain::shared::ids::{MaintenanceWindowId, MonitorId, UserId};
use domain::{Incident, MaintenanceWindow, Monitor, notification::NotificationEvent};
use tracing::{error, info};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Handles opening and closing incidents, and notifying a monitor's
/// notification channels when either happens.
#[derive(Clone)]
pub struct IncidentManager {
    repository: Arc<dyn IncidentRepository>,
    id_generator: Arc<dyn IdGenerator>,
    notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    notification_service: Arc<dyn NotificationService>,
    maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
}

#[derive(Clone)]
struct NoopMaintenanceWindowRepository;

#[async_trait]
impl MaintenanceWindowRepository for NoopMaintenanceWindowRepository {
    async fn create(&self, _window: &MaintenanceWindow) -> Result<()> {
        Ok(())
    }

    async fn delete_for_user(&self, _id: MaintenanceWindowId, _user_id: UserId) -> Result<bool> {
        Ok(false)
    }

    async fn find_by_id_for_user(
        &self,
        _id: MaintenanceWindowId,
        _user_id: UserId,
    ) -> Result<Option<MaintenanceWindow>> {
        Ok(None)
    }

    async fn list_by_monitor_for_user(
        &self,
        _monitor_id: MonitorId,
        _user_id: UserId,
    ) -> Result<Option<Vec<MaintenanceWindow>>> {
        Ok(None)
    }

    async fn list_active_by_monitor(
        &self,
        _monitor_id: MonitorId,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<MaintenanceWindow>> {
        Ok(vec![])
    }
}

impl IncidentManager {
    pub fn new(
        repository: Arc<dyn IncidentRepository>,
        id_generator: Arc<dyn IdGenerator>,
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
        notification_service: Arc<dyn NotificationService>,
    ) -> Self {
        Self::new_with_maintenance_window(
            repository,
            id_generator,
            notification_channel_repository,
            notification_service,
            Arc::new(NoopMaintenanceWindowRepository),
        )
    }

    pub fn new_with_maintenance_window(
        repository: Arc<dyn IncidentRepository>,
        id_generator: Arc<dyn IdGenerator>,
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
        notification_service: Arc<dyn NotificationService>,
        maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
    ) -> Self {
        Self {
            repository,
            id_generator,
            notification_channel_repository,
            notification_service,
            maintenance_window_repository,
        }
    }

    /// Yana buɗe wani sabon incident ga `monitor` idan babu wani open
    /// incident a halin yanzu. Idan akwai active maintenance window a
    /// lokacin, incident ɗin zai kasance mara notification (silent) kuma
    /// flagged `occurred_during_maintenance = true`.
    pub async fn open_if_needed(&self, monitor: &Monitor) -> Result<()> {
        let existing = self.repository.find_open_by_monitor(monitor.id).await?;

        if existing.is_none() {
            let mut incident =
                Incident::new(self.id_generator.next_incident_id().await, monitor.id);

            // Check maintenance windows
            let now = chrono::Utc::now();
            let active_windows = self
                .maintenance_window_repository
                .list_active_by_monitor(monitor.id, now)
                .await?;

            let in_maintenance = !active_windows.is_empty();
            if in_maintenance {
                incident.mark_occurred_during_maintenance();
            }

            self.repository.save(&incident).await?;

            info!(
                monitor_id = %monitor.id.0,
                incident_id = %incident.id.0,
                in_maintenance = in_maintenance,
                "incident opened"
            );

            // Kada a tura notification idan muna cikin maintenance window.
            if !in_maintenance {
                self.notify_channels(monitor, NotificationEvent::MonitorDown(monitor.clone()))
                    .await;
            }
        }

        Ok(())
    }

    pub async fn resolve_if_needed(&self, monitor: &Monitor) -> Result<()> {
        if let Some(mut incident) = self.repository.find_open_by_monitor(monitor.id).await? {
            incident.resolve();
            self.repository.update(&incident).await?;

            info!(
                monitor_id = %monitor.id.0,
                incident_id = %incident.id.0,
                "incident resolved"
            );

            self.notify_channels(
                monitor,
                NotificationEvent::MonitorRecovered(monitor.clone()),
            )
            .await;
        }

        Ok(())
    }

    /// Sends `event` to every enabled notification channel configured for
    /// `monitor`. Deliberately never returns an error and is always called
    /// AFTER the incident itself has already been saved/updated above.
    async fn notify_channels(&self, monitor: &Monitor, event: NotificationEvent) {
        let channels = match self
            .notification_channel_repository
            .list_enabled_by_monitor(monitor.id)
            .await
        {
            Ok(channels) => channels,
            Err(err) => {
                error!(
                    monitor_id = %monitor.id.0,
                    error = %err,
                    "failed to load notification channels"
                );
                return;
            }
        };

        for channel in &channels {
            if let Err(err) = self.notification_service.notify(channel, &event).await {
                error!(
                    monitor_id = %monitor.id.0,
                    channel_id = %channel.id.0,
                    error = %err,
                    "failed to send notification"
                );
            }
        }
    }
}
