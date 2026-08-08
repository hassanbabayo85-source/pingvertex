use std::{error::Error, sync::Arc};

use application::ports::{IdGenerator, IncidentRepository};
use domain::Incident;
use tracing::info;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Handles opening and closing incidents.
#[derive(Clone)]
pub struct IncidentManager {
    repository: Arc<dyn IncidentRepository>,
    id_generator: Arc<dyn IdGenerator>,
}

impl IncidentManager {
    pub fn new(
        repository: Arc<dyn IncidentRepository>,
        id_generator: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            repository,
            id_generator,
        }
    }

    pub async fn open_if_needed(&self, monitor_id: domain::shared::ids::MonitorId) -> Result<()> {
        let existing = self.repository.find_open_by_monitor(monitor_id).await?;

        if existing.is_none() {
            let incident = Incident::new(self.id_generator.next_incident_id().await, monitor_id);

            self.repository.save(&incident).await?;

            info!(
                monitor_id = %monitor_id.0,
                incident_id = %incident.id.0,
                "incident opened"
            );
        }

        Ok(())
    }

    pub async fn resolve_if_needed(
        &self,
        monitor_id: domain::shared::ids::MonitorId,
    ) -> Result<()> {
        if let Some(mut incident) = self.repository.find_open_by_monitor(monitor_id).await? {
            incident.resolve();
            self.repository.update(&incident).await?;

            info!(
                monitor_id = %monitor_id.0,
                incident_id = %incident.id.0,
                "incident resolved"
            );
        }

        Ok(())
    }
}
