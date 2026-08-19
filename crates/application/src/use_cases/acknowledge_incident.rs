use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use domain::shared::types::AuthenticatedUser;
use domain::{AuditAction, AuditLogEntry, AuditLogEntryId, Incident};
use serde_json::json;

use crate::{
    commands::AcknowledgeIncident,
    ports::{AuditLogEntryRepository, IncidentRepository, MonitorRepository},
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
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl AcknowledgeIncidentUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        incident_repository: Arc<dyn IncidentRepository>,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            monitor_repository,
            incident_repository,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: AcknowledgeIncident,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Incident, AcknowledgeIncidentError> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

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
            .find_by_id_for_user(incident.monitor_id, user_id)
            .await
            .map_err(AcknowledgeIncidentError::Other)?
            .ok_or(AcknowledgeIncidentError::NotFound)?;

        if incident.resolved_at.is_some() {
            return Err(AcknowledgeIncidentError::AlreadyResolved);
        }

        let now = Utc::now();
        incident.acknowledge(user_id, now);

        self.incident_repository
            .update(&incident)
            .await
            .map_err(AcknowledgeIncidentError::Other)?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::IncidentAcknowledged,
            target_id: incident.id.0.to_string(),
            metadata: json!({
                "monitor_id": incident.monitor_id.0.to_string(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(AcknowledgeIncidentError::Other)?;

        Ok(incident)
    }
}
