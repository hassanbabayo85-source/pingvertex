use std::{error::Error, sync::Arc};

use application::ports::{
    IdGenerator, IncidentRepository, NotificationChannelRepository, NotificationService,
};
use domain::{Incident, Monitor, notification::NotificationEvent};
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
}

impl IncidentManager {
    pub fn new(
        repository: Arc<dyn IncidentRepository>,
        id_generator: Arc<dyn IdGenerator>,
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
        notification_service: Arc<dyn NotificationService>,
    ) -> Self {
        Self {
            repository,
            id_generator,
            notification_channel_repository,
            notification_service,
        }
    }

    /// Yana buɗe wani sabon incident ga `monitor` idan babu wani open
    /// incident a halin yanzu, sannan ya sanar da duk notification
    /// channels ɗin monitor ɗin idan an buɗe sabon incident.
    ///
    /// SEC-010: wannan aikin yana yin "duba-sannan-ƙirƙira" (check-then-act)
    /// wanda BA atomic ba ne a matakin application — idan worker replicas
    /// fiye da ɗaya suka gudana lokaci guda, duka biyun na iya wuce
    /// `find_open_by_monitor` a lokaci guda su ga "babu open incident".
    ///
    /// Duk da haka, tun bayan migration `0009_add_unique_open_incident_per_monitor`,
    /// wannan BA ZAI SAKE haifar da incidents na kwafi (duplicates) ba: layin
    /// database (`PostgresIncidentRepository::save`) yana da partial unique
    /// index + `ON CONFLICT DO NOTHING`, don haka koda application-level
    /// check ɗin ya "kasa" saboda race, kawai incident ɗaya ne zai ƙare a
    /// cikin database — na biyun zai kasance shiru (no-op). Wannan check a
    /// nan har yanzu yana da amfani domin ya rage yawan buƙatu marasa
    /// muhimmanci zuwa database a yanayi na yau da kullum (babu race).
    pub async fn open_if_needed(&self, monitor: &Monitor) -> Result<()> {
        let existing = self.repository.find_open_by_monitor(monitor.id).await?;

        if existing.is_none() {
            let incident = Incident::new(self.id_generator.next_incident_id().await, monitor.id);

            self.repository.save(&incident).await?;

            info!(
                monitor_id = %monitor.id.0,
                incident_id = %incident.id.0,
                "incident opened"
            );

            self.notify_channels(monitor, NotificationEvent::MonitorDown(monitor.clone()))
                .await;
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
    /// AFTER the incident itself has already been saved/updated above: a
    /// failing or slow email/webhook provider must never prevent the
    /// incident from being recorded, and must never stop the scheduler
    /// sweep from moving on to other monitors (see `scheduler.rs`, which
    /// already only logs errors from `open_if_needed`/`resolve_if_needed`
    /// rather than propagating them further).
    ///
    /// Runs channel sends sequentially rather than via `tokio::spawn`: the
    /// scheduler already runs every monitor's check (check + this
    /// incident/notification handling) as its own concurrent task in a
    /// `JoinSet` (see `scheduler.rs::process_sweep`), and a monitor's
    /// `last_checked_at` is recorded before that task is spawned — so a
    /// slow notification here delays neither sibling monitors' checks nor
    /// this monitor's next scheduled check. Spawning a second, nested
    /// background task per channel on top of that would add complexity
    /// (and test non-determinism) without a corresponding benefit.
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
