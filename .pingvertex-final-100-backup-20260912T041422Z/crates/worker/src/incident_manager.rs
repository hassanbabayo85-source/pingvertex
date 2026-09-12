use std::{error::Error, sync::Arc};

use application::ports::{
    IdGenerator, IncidentRepository, MaintenanceWindowRepository, NotificationChannelRepository,
    NotificationOutboxRepository, NotificationService, PUBLIC_STATUS_LIST_CACHE_KEY,
    StatusPageCache, StatusPageRepository,
};
use async_trait::async_trait;
use domain::shared::ids::{IncidentId, MonitorId};
use domain::{
    Incident, MaintenanceWindow, Monitor, NotificationOutboxEntry, NotificationOutboxEntryId,
    NotificationOutboxEventKind, notification::NotificationEvent,
};
use tracing::{error, info, warn};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
struct StatusPageInvalidation {
    status_page_repository: Arc<dyn StatusPageRepository>,
    status_page_cache: Arc<dyn StatusPageCache>,
}

#[derive(Clone)]
pub struct IncidentManager {
    repository: Arc<dyn IncidentRepository>,
    id_generator: Arc<dyn IdGenerator>,
    notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    notification_service: Arc<dyn NotificationService>,
    maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
    status_page_invalidation: Option<StatusPageInvalidation>,
    notification_outbox_repository: Option<Arc<dyn NotificationOutboxRepository>>,
}

#[derive(Clone)]
struct NoopMaintenanceWindowRepository;

#[async_trait]
impl MaintenanceWindowRepository for NoopMaintenanceWindowRepository {
    async fn create(&self, _window: &MaintenanceWindow) -> Result<()> {
        Ok(())
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
            status_page_invalidation: None,
            notification_outbox_repository: None,
        }
    }

    pub fn with_status_page_cache_invalidation(
        mut self,
        status_page_repository: Arc<dyn StatusPageRepository>,
        status_page_cache: Arc<dyn StatusPageCache>,
    ) -> Self {
        self.status_page_invalidation = Some(StatusPageInvalidation {
            status_page_repository,
            status_page_cache,
        });
        self
    }

    pub fn with_notification_outbox(
        mut self,
        notification_outbox_repository: Arc<dyn NotificationOutboxRepository>,
    ) -> Self {
        self.notification_outbox_repository = Some(notification_outbox_repository);
        self
    }

    pub async fn open_if_needed(&self, monitor: &Monitor) -> Result<()> {
        let existing = self.repository.find_open_by_monitor(monitor.id).await?;

        if existing.is_none() {
            let mut incident =
                Incident::new(self.id_generator.next_incident_id().await, monitor.id);

            let now = chrono::Utc::now();
            let active_windows = self
                .maintenance_window_repository
                .list_active_by_monitor(monitor.id, now)
                .await?;

            let in_maintenance = !active_windows.is_empty();
            if in_maintenance {
                incident.mark_occurred_during_maintenance();
            }

            let created = self.repository.save_and_report(&incident).await?;

            if created {
                info!(
                    monitor_id = %monitor.id.0,
                    incident_id = %incident.id.0,
                    in_maintenance = in_maintenance,
                    "incident opened"
                );

                self.invalidate_status_pages_for(monitor).await;

                if !in_maintenance {
                    self.notify_channels(
                        monitor,
                        NotificationEvent::MonitorDown(monitor.clone()),
                        incident.id,
                    )
                    .await;
                }
            }
        }

        Ok(())
    }

    pub async fn resolve_if_needed(&self, monitor: &Monitor) -> Result<()> {
        if let Some(mut incident) = self.repository.find_open_by_monitor(monitor.id).await? {
            incident.resolve();
            let resolved = self.repository.resolve_if_open(&incident).await?;

            if resolved {
                info!(
                    monitor_id = %monitor.id.0,
                    incident_id = %incident.id.0,
                    "incident resolved"
                );

                self.invalidate_status_pages_for(monitor).await;

                self.notify_channels(
                    monitor,
                    NotificationEvent::MonitorRecovered(monitor.clone()),
                    incident.id,
                )
                .await;
            }
        }

        Ok(())
    }

    async fn invalidate_status_pages_for(&self, monitor: &Monitor) {
        let Some(invalidation) = &self.status_page_invalidation else {
            return;
        };

        match invalidation
            .status_page_repository
            .find_all_referencing_monitor(monitor.id)
            .await
        {
            Ok(status_pages) => {
                for status_page in status_pages {
                    let cache_key = format!("status_page:id:{}", status_page.id.0);
                    if let Err(err) = invalidation.status_page_cache.invalidate(&cache_key).await {
                        warn!(
                            monitor_id = %monitor.id.0,
                            status_page_id = %status_page.id.0,
                            error = %err,
                            "failed to invalidate cached status page after incident change"
                        );
                    }
                }
            }
            Err(err) => {
                warn!(
                    monitor_id = %monitor.id.0,
                    error = %err,
                    "failed to look up status pages referencing monitor for cache invalidation"
                );
            }
        }

        if let Err(err) = invalidation
            .status_page_cache
            .invalidate(PUBLIC_STATUS_LIST_CACHE_KEY)
            .await
        {
            warn!(
                monitor_id = %monitor.id.0,
                error = %err,
                "failed to invalidate cached public status list after incident change"
            );
        }
    }

    pub async fn touch_last_seen_if_open(&self, monitor: &Monitor) -> Result<()> {
        self.repository
            .touch_last_seen(monitor.id, chrono::Utc::now())
            .await
    }

    async fn notify_channels(
        &self,
        monitor: &Monitor,
        event: NotificationEvent,
        incident_id: IncidentId,
    ) {
        if let Some(outbox) = &self.notification_outbox_repository {
            self.enqueue_outbox_notifications(outbox, monitor, event, incident_id)
                .await;
            return;
        }

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
            let event_kind = match &event {
                NotificationEvent::MonitorDown(_) => "monitor_down",
                NotificationEvent::MonitorRecovered(_) => "monitor_recovered",
                NotificationEvent::Test(_) => "test",
            };
            let request_id = format!("incident:{}:{}:{}", incident_id.0, channel.id.0, event_kind);
            if let Err(err) = self
                .notification_service
                .notify_with_request_id(channel, &event, &request_id)
                .await
            {
                error!(
                    monitor_id = %monitor.id.0,
                    channel_id = %channel.id.0,
                    error = %err,
                    "failed to send notification"
                );
            }
        }
    }

    async fn enqueue_outbox_notifications(
        &self,
        outbox: &Arc<dyn NotificationOutboxRepository>,
        monitor: &Monitor,
        event: NotificationEvent,
        incident_id: IncidentId,
    ) {
        let event_kind = match event {
            NotificationEvent::MonitorDown(_) => NotificationOutboxEventKind::MonitorDown,
            NotificationEvent::MonitorRecovered(_) => NotificationOutboxEventKind::MonitorRecovered,
            NotificationEvent::Test(_) => NotificationOutboxEventKind::Test,
        };

        let payload = match serde_json::to_string(monitor) {
            Ok(payload) => payload,
            Err(err) => {
                error!(
                    monitor_id = %monitor.id.0,
                    error = %err,
                    "failed to serialize monitor for notification outbox"
                );
                return;
            }
        };

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
                    "failed to load notification channels for outbox"
                );
                return;
            }
        };

        for channel in channels {
            let idempotency_key = format!(
                "incident:{}:{}:{}",
                incident_id.0,
                channel.id.0,
                event_kind.as_str()
            );

            let entry = NotificationOutboxEntry::new(
                NotificationOutboxEntryId::new(),
                channel.id,
                event_kind,
                payload.clone(),
                idempotency_key,
                chrono::Utc::now(),
            );

            if let Err(err) = outbox.enqueue(&entry).await {
                error!(
                    monitor_id = %monitor.id.0,
                    channel_id = %channel.id.0,
                    incident_id = %incident_id.0,
                    error = %err,
                    "failed to enqueue incident notification"
                );
            }
        }
    }
}
