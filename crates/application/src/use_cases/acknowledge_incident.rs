use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use domain::Incident;

use crate::{
    commands::AcknowledgeIncident,
    ports::{IncidentRepository, MonitorRepository},
};

/// Error returned by `AcknowledgeIncidentUseCase`.
#[derive(Debug)]
pub enum AcknowledgeIncidentError {
    /// Incident doesn't exist, or exists but belongs to a monitor the user
    /// doesn't own. Deliberately the same outward result either way.
    NotFound,
    /// Incident is already resolved — acknowledgement only makes sense for
    /// open incidents.
    AlreadyResolved,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for AcknowledgeIncidentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "incident not found or access denied"),
            Self::AlreadyResolved => write!(f, "incident is already resolved"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for AcknowledgeIncidentError {}

/// Acknowledges an incident, verifying that the user owns the monitor
/// associated with that incident.
pub struct AcknowledgeIncidentUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    incident_repository: Arc<dyn IncidentRepository>,
}

impl AcknowledgeIncidentUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        incident_repository: Arc<dyn IncidentRepository>,
    ) -> Self {
        Self {
            monitor_repository,
            incident_repository,
        }
    }

    pub async fn execute(
        &self,
        command: AcknowledgeIncident,
    ) -> Result<Incident, AcknowledgeIncidentError> {
        let mut incident = self
            .incident_repository
            .find_by_id(command.incident_id)
            .await
            .map_err(AcknowledgeIncidentError::Other)?
            .ok_or(AcknowledgeIncidentError::NotFound)?;

        // Verify ownership via the monitor repository.
        // If the monitor doesn't exist or isn't owned by this user,
        // we return NotFound (same outward result as incident not found).
        self.monitor_repository
            .find_by_id_for_user(incident.monitor_id, command.user_id)
            .await
            .map_err(AcknowledgeIncidentError::Other)?
            .ok_or(AcknowledgeIncidentError::NotFound)?;

        if incident.resolved_at.is_some() {
            return Err(AcknowledgeIncidentError::AlreadyResolved);
        }

        let now = Utc::now();
        incident.acknowledge(command.user_id, now);

        self.incident_repository
            .update(&incident)
            .await
            .map_err(AcknowledgeIncidentError::Other)?;

        Ok(incident)
    }
}
