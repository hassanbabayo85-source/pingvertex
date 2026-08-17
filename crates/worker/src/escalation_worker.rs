use std::{error::Error, sync::Arc, time::Duration};

use application::ports::{
    EscalationPolicyRepository, IncidentRepository, MonitorRepository,
    NotificationChannelRepository, NotificationService,
};
use domain::{Incident, notification::NotificationEvent};
use tracing::{debug, error, info, warn};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Runs a periodic sweep that checks open, unacknowledged incidents and
/// sends escalation notifications according to each monitor's effective
/// escalation policy.
#[derive(Clone)]
pub struct EscalationWorker {
    incident_repository: Arc<dyn IncidentRepository>,
    monitor_repository: Arc<dyn MonitorRepository>,
    escalation_policy_repository: Arc<dyn EscalationPolicyRepository>,
    notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    notification_service: Arc<dyn NotificationService>,
}

impl EscalationWorker {
    pub fn new(
        incident_repository: Arc<dyn IncidentRepository>,
        monitor_repository: Arc<dyn MonitorRepository>,
        escalation_policy_repository: Arc<dyn EscalationPolicyRepository>,
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
        notification_service: Arc<dyn NotificationService>,
    ) -> Self {
        Self {
            incident_repository,
            monitor_repository,
            escalation_policy_repository,
            notification_channel_repository,
            notification_service,
        }
    }

    /// Runs one sweep. Returns Ok(()) even if individual sends fail; errors
    /// are logged and the sweep continues.
    pub async fn run_once(&self) -> Result<()> {
        let incidents = self.incident_repository.list_open_unacknowledged().await?;

        debug!(count = incidents.len(), "escalation sweep starting");

        for mut incident in incidents {
            if let Err(err) = self.process_incident(&mut incident).await {
                error!(
                    incident_id = %incident.id.0,
                    error = %err,
                    "failed to process escalation for incident"
                );
                continue;
            }
        }

        Ok(())
    }

    async fn process_incident(&self, incident: &mut Incident) -> Result<()> {
        let monitor = match self
            .monitor_repository
            .find_by_id(incident.monitor_id)
            .await?
        {
            Some(m) => m,
            None => {
                warn!(
                    monitor_id = %incident.monitor_id.0,
                    "monitor not found for incident during escalation"
                );
                return Ok(());
            }
        };

        let policy = match self
            .escalation_policy_repository
            .find_effective(monitor.id, monitor.user_id)
            .await?
        {
            Some(p) => p,
            None => {
                debug!(
                    monitor_id = %monitor.id.0,
                    "no escalation policy for monitor; skipping"
                );
                return Ok(());
            }
        };

        let step_index = incident.escalation_step_index as usize;
        if step_index >= policy.steps.len() {
            debug!(
                incident_id = %incident.id.0,
                step_index = step_index,
                "all escalation steps already sent"
            );
            return Ok(());
        }

        let step = &policy.steps[step_index];
        let now = chrono::Utc::now();
        let base_time = if step_index == 0 {
            incident.started_at
        } else {
            incident
                .last_escalation_at
                .expect("last_escalation_at must be set after step 0")
        };

        let delay = chrono::Duration::minutes(step.delay_minutes as i64);
        if now < base_time + delay {
            debug!(
                incident_id = %incident.id.0,
                step_index = step_index,
                "delay not elapsed yet"
            );
            return Ok(());
        }

        // Fetch channels for this monitor (enabled only) and find the
        // channel specified in the step.
        let channels = self
            .notification_channel_repository
            .list_enabled_by_monitor(monitor.id)
            .await?;

        let channel = channels.iter().find(|c| c.id == step.channel_id);
        if channel.is_none() {
            warn!(
                incident_id = %incident.id.0,
                channel_id = %step.channel_id.0,
                "escalation channel not found or disabled"
            );
            return Ok(());
        }

        let channel = channel.unwrap();

        // Send escalation notification (reusing MonitorDown event).
        if let Err(err) = self
            .notification_service
            .notify(channel, &NotificationEvent::MonitorDown(monitor.clone()))
            .await
        {
            error!(
                incident_id = %incident.id.0,
                channel_id = %channel.id.0,
                error = %err,
                "failed to send escalation notification"
            );
            return Ok(());
        }

        info!(
            incident_id = %incident.id.0,
            step_index = step_index,
            channel_id = %channel.id.0,
            "sent escalation notification"
        );

        // Advance escalation progress
        incident.escalation_step_index = step_index as u32 + 1;
        incident.last_escalation_at = Some(now);
        incident.updated_at = now;
        self.incident_repository.update(incident).await?;

        Ok(())
    }

    /// Runs forever, sleeping 60 seconds between sweeps.
    pub async fn run_forever(&self) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(err) = self.run_once().await {
                error!(error = %err, "escalation sweep failed");
            }
        }
    }
}
